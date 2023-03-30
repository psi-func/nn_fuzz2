#![allow(dead_code)]
/// Llmp extension to provide nn predicts over network
///
use std::marker::PhantomData;

use std::ops::{Deref, DerefMut};
use std::thread;
use std::time::Duration;

use libafl::bolts::llmp::{Flags, Tag, LLMP_FLAG_COMPRESSED, LLMP_FLAG_INITIALIZED};
use libafl::bolts::shmem::ShMemProvider;
use libafl::events::{BrokerEventResult, Event, LlmpEventManager};

use libafl::events::EventRestarter;
use libafl::inputs::{Input, UsesInput};
use libafl::monitors::Monitor;
use libafl::prelude::{
    ClientId, EventConfig, EventFirer, EventManager, EventManagerId, EventProcessor, Executor,
    GzipCompressor, HasEventManagerId, HasObservers, LlmpBroker, LlmpClient, LlmpClientDescription,
    LlmpMsgHookResult, LlmpSharedMap, ProgressReporter, ShMem, ShMemDescription, StateRestorer,
};
use libafl::state::{HasClientPerfMonitor, HasExecutions, HasMetadata, UsesState};
use libafl::{Error, EvaluatorObservers, ExecutionProcessor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::channel;

use crate::connector::server::run_service;

const LLMP_TAG_EVENT_TO_BOTH: Tag = 0x002B_0741;

/// The minimum buffer size at which to compress LLMP IPC messages.
const COMPRESS_THRESHOLD: usize = 1024;

#[derive(Debug)]
pub struct RestartingNnEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider + 'static,
{
    /// The embedded llmp event manager
    mgr: NNEventManager<S, SP>,
    /// The staterestorer to serialize the state for the next runner
    staterestorer: StateRestorer<SP>,
}

impl<S, SP> UsesState for RestartingNnEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider + 'static,
{
    type State = S;
}

impl<S, SP> ProgressReporter for RestartingNnEventManager<S, SP>
where
    S: UsesInput + HasExecutions + HasClientPerfMonitor + HasMetadata,
    SP: ShMemProvider + 'static,
{
}

impl<S, SP> EventFirer for RestartingNnEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider + 'static,
{
    fn fire(
        &mut self,
        state: &mut Self::State,
        event: libafl::prelude::Event<<Self::State as UsesInput>::Input>,
    ) -> Result<(), Error> {
        self.mgr.fire(state, event)
    }

    fn configuration(&self) -> EventConfig {
        self.mgr.configuration()
    }
}

impl<S, SP> EventRestarter for RestartingNnEventManager<S, SP>
where
    S: UsesInput + HasExecutions + HasClientPerfMonitor + Serialize,
    SP: ShMemProvider,
{
    #[inline]
    fn await_restart_safe(&mut self) {
        self.mgr.await_restart_safe();
    }

    fn on_restart(&mut self, state: &mut S) -> Result<(), Error> {
        self.staterestorer.reset();
        self.staterestorer.save(&(state, &self.mgr.describe()?))
    }
}

impl<E, S, SP, Z> EventProcessor<E, Z> for RestartingNnEventManager<S, SP>
where
    E: HasObservers<State = S> + Executor<LlmpEventManager<S, SP>, Z>,
    for<'a> E::Observers: Deserialize<'a>,
    S: UsesInput + HasExecutions + HasClientPerfMonitor,
    SP: ShMemProvider + 'static,
    Z: EvaluatorObservers<E::Observers, State = S> + ExecutionProcessor<E::Observers>, //CE: CustomEvent<I>,
{
    fn process(&mut self, fuzzer: &mut Z, state: &mut S, executor: &mut E) -> Result<usize, Error> {
        self.mgr.process(fuzzer, state, executor)
    }
}

impl<E, S, SP, Z> EventManager<E, Z> for RestartingNnEventManager<S, SP>
where
    E: HasObservers<State = S> + Executor<LlmpEventManager<S, SP>, Z>,
    for<'a> E::Observers: Deserialize<'a>,
    S: UsesInput + HasExecutions + HasClientPerfMonitor + HasMetadata + Serialize,
    SP: ShMemProvider + 'static,
    Z: EvaluatorObservers<E::Observers, State = S> + ExecutionProcessor<E::Observers>, //CE: CustomEvent<I>,
{
}

impl<S, SP> HasEventManagerId for RestartingNnEventManager<S, SP>
where
    S: UsesInput + Serialize,
    SP: ShMemProvider + 'static,
{
    fn mgr_id(&self) -> EventManagerId {
        self.mgr.mgr_id()
    }
}

impl<S, SP> RestartingNnEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider + 'static,
{
    /// Create a new runner, the executed child doing the nn connection loop.
    pub fn new(mgr: NNEventManager<S, SP>, staterestorer: StateRestorer<SP>) -> Self {
        Self { mgr, staterestorer }
    }

    /// Get the staterestorer
    pub fn staterestorer(&self) -> &StateRestorer<SP> {
        &self.staterestorer
    }

    /// Get the staterestorer (mutable)
    pub fn staterestorer_mut(&mut self) -> &mut StateRestorer<SP> {
        &mut self.staterestorer
    }
}

#[derive(Debug)]
pub struct NNEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider + 'static,
{
    llmp: LlmpClient<SP>,

    compressor: GzipCompressor,
    configuration: EventConfig,
    phantom: PhantomData<S>,
}

impl<S, SP> UsesState for NNEventManager<S, SP>
where
    SP: ShMemProvider + 'static,
    S: UsesInput,
{
    type State = S;
}

impl<S, SP> Drop for NNEventManager<S, SP>
where
    SP: ShMemProvider + 'static,
    S: UsesInput,
{
    /// LLMP clients will have to wait until their pages are mapped by somebody
    fn drop(&mut self) {
        self.await_restart_safe();
    }
}

impl<S, SP> NNEventManager<S, SP>
where
    S: UsesInput + HasExecutions,
    SP: ShMemProvider + 'static,
{
    pub fn new_on_port(
        shmem_provider: SP,
        port: u16,
        configuration: EventConfig,
    ) -> Result<Self, Error> {
        Ok(Self {
            llmp: LlmpClient::create_attach_to_tcp(shmem_provider, port)?,
            compressor: GzipCompressor::new(COMPRESS_THRESHOLD),
            configuration,
            phantom: PhantomData,
        })
    }

    pub fn existing_client_from_env(
        shmem_provider: SP,
        env_name: &str,
        configuration: EventConfig,
    ) -> Result<Self, Error> {
        Ok(Self {
            llmp: LlmpClient::on_existing_from_env(shmem_provider, env_name)?,
            compressor: GzipCompressor::new(COMPRESS_THRESHOLD),
            configuration,
            phantom: PhantomData,
        })
    }

    /// Create an existing client from description
    pub fn existing_client_from_description(
        shmem_provider: SP,
        description: &LlmpClientDescription,
        configuration: EventConfig,
    ) -> Result<Self, Error> {
        Ok(Self {
            llmp: LlmpClient::existing_client_from_description(shmem_provider, description)?,
            compressor: GzipCompressor::new(COMPRESS_THRESHOLD),
            configuration,
            phantom: PhantomData,
        })
    }

    /// Write the config for a client [`EventManager`] to env vars, a new client can reattach using [`LlmpEventManager::existing_client_from_env()`].
    pub fn to_env(&self, env_name: &str) {
        self.llmp.to_env(env_name).unwrap();
    }

    pub fn describe(&self) -> Result<LlmpClientDescription, Error> {
        self.llmp.describe()
    }
}

impl<S, SP> EventRestarter for NNEventManager<S, SP>
where
    SP: ShMemProvider,
    S: UsesInput,
{
    /// The llmp client needs to wait until a broker mapped all pages, before shutting down.
    /// Otherwise, the OS may already have removed the shared maps,
    fn await_restart_safe(&mut self) {
        // wait until we can drop the message safely.
        self.llmp.await_safe_to_unmap_blocking();
    }
}

impl<S, SP> EventFirer for NNEventManager<S, SP>
where
    SP: ShMemProvider + 'static,
    S: UsesInput,
{
    fn fire(
        &mut self,
        _state: &mut Self::State,
        event: libafl::prelude::Event<<Self::State as UsesInput>::Input>,
    ) -> Result<(), Error> {
        let serialized = postcard::to_allocvec(&event)?;
        let flags: Flags = LLMP_FLAG_INITIALIZED;

        match self.compressor.compress(&serialized)? {
            Some(comp_buf) => {
                self.llmp.send_buf_with_flags(
                    LLMP_TAG_EVENT_TO_BOTH,
                    flags | LLMP_FLAG_COMPRESSED,
                    &comp_buf,
                )?;
            }
            None => {
                self.llmp.send_buf(LLMP_TAG_EVENT_TO_BOTH, &serialized)?;
            }
        }
        Ok(())
    }

    fn configuration(&self) -> EventConfig {
        self.configuration
    }
}

impl<E, S, SP, Z> EventProcessor<E, Z> for NNEventManager<S, SP>
where
    SP: ShMemProvider,
    S: UsesInput,
    E: HasObservers<State = S>,
{
    fn process(
        &mut self,
        _fuzzer: &mut Z,
        _state: &mut Self::State,
        _executor: &mut E,
    ) -> Result<usize, Error> {
        todo!()
    }
}

impl<S, SP> HasEventManagerId for NNEventManager<S, SP>
where
    S: UsesInput,
    SP: ShMemProvider,
{
    fn mgr_id(&self) -> EventManagerId {
        EventManagerId {
            id: self.llmp.sender.id as usize,
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
struct LlmpNnBroker<SP: ShMemProvider + 'static>(LlmpBroker<SP>);

impl<SP> Deref for LlmpNnBroker<SP>
where
    SP: ShMemProvider + 'static,
{
    type Target = LlmpBroker<SP>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<SP> DerefMut for LlmpNnBroker<SP>
where
    SP: ShMemProvider + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<SP> LlmpNnBroker<SP>
where
    SP: ShMemProvider + 'static,
{
    pub fn create_attach_to_tcp(shmem_provider: SP, port: u16) -> Result<Self, Error> {
        Ok(LlmpNnBroker(LlmpBroker::create_attach_to_tcp(
            shmem_provider,
            port,
        )?))
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn spawn_client(&mut self, port: u16) -> Result<(), Error> {
        let map_description = Self::nn_thread_on(
            port,
            self.0.llmp_clients.len() as ClientId,
            &self
                .0
                .llmp_out
                .out_shmems
                .first()
                .unwrap()
                .shmem
                .description(),
        )?;

        let new_shmem = LlmpSharedMap::existing(
            self.0
                .shmem_provider()
                .shmem_from_description(map_description)?,
        );

        {
            self.0.register_client(new_shmem);
        }

        Ok(())
    }

    fn nn_thread_on(
        port: u16,
        client_id: ClientId,
        broker_shmem_description: &ShMemDescription,
    ) -> Result<ShMemDescription, Error> {
        let broker_shmem_description = *broker_shmem_description;

        let (send, recv) = channel();

        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(3)
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    run_service::<SP>(send, broker_shmem_description, client_id, port).await;
                });
        });

        

        recv.blocking_recv().map_err(|_| {
            Error::unknown("Error launching background thread for nn communication".to_string())
        })
    }
}

pub struct LlmpNnEventBroker<I, MT, SP>
where
    I: Input,
    MT: Monitor,
    SP: ShMemProvider + 'static,
{
    monitor: MT,
    llmp: LlmpNnBroker<SP>,
    compressor: GzipCompressor,
    phantom: PhantomData<I>,
}

impl<I, MT, SP> LlmpNnEventBroker<I, MT, SP>
where
    I: Input,
    SP: ShMemProvider + 'static,
    MT: Monitor,
{
    pub fn new_on_port(shmem_provider: SP, monitor: MT, port: u16) -> Result<Self, Error> {
        Ok(Self {
            monitor,
            llmp: LlmpNnBroker::create_attach_to_tcp(shmem_provider, port)?,
            compressor: GzipCompressor::new(COMPRESS_THRESHOLD),
            phantom: PhantomData,
        })
    }

    pub fn spawn_client(&mut self, port: u16) -> Result<(), Error> {
        self.llmp.spawn_client(port)
    }

    pub fn broker_loop(&mut self) -> Result<(), Error> {
        let monitor = &mut self.monitor;
        let compressor = &self.compressor;
        self.llmp.loop_forever(
            &mut |client_id: u32, tag: Tag, _flags: Flags, msg: &[u8]| {
                if tag == LLMP_TAG_EVENT_TO_BOTH {
                    let compressed;

                    let event_bytes = if _flags & LLMP_FLAG_COMPRESSED == LLMP_FLAG_COMPRESSED {
                        compressed = compressor.decompress(msg)?;
                        &compressed
                    } else {
                        msg
                    };
                    let event: Event<I> = postcard::from_bytes(event_bytes)?;
                    match Self::handle_in_broker(monitor, client_id, &event)? {
                        BrokerEventResult::Forward => Ok(LlmpMsgHookResult::ForwardToClients),
                        BrokerEventResult::Handled => Ok(LlmpMsgHookResult::Handled),
                    }
                } else {
                    Ok(LlmpMsgHookResult::ForwardToClients)
                }
            },
            Some(Duration::from_millis(5)),
        );

        Ok(())
    }

    fn handle_in_broker(
        monitor: &mut MT,
        client_id: u32,
        event: &Event<I>,
    ) -> Result<BrokerEventResult, Error> {
        match &event {
            Event::NewTestcase {
                input: _,
                client_config: _,
                exit_kind: _,
                corpus_size,
                observers_buf: _,
                time,
                executions,
            } => {
                let client = monitor.client_stats_mut_for(client_id);
                client.update_corpus_size(*corpus_size as u64);
                client.update_executions(*executions as u64, *time);
                monitor.display(event.name().to_string(), client_id);
                Ok(BrokerEventResult::Forward)
            }
            Event::UpdateExecStats {
                time,
                executions,
                phantom: _,
            } => {
                // TODO: The monitor buffer should be added on client add.
                let client = monitor.client_stats_mut_for(client_id);
                client.update_executions(*executions as u64, *time);
                monitor.display(event.name().to_string(), client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::UpdateUserStats {
                name,
                value,
                phantom: _,
            } => {
                let client = monitor.client_stats_mut_for(client_id);
                client.update_user_stats(name.clone(), value.clone());
                monitor.display(event.name().to_string(), client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::Objective { objective_size } => {
                let client = monitor.client_stats_mut_for(client_id);
                client.update_objective_size(*objective_size as u64);
                monitor.display(event.name().to_string(), client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::Log {
                severity_level,
                message,
                phantom: _,
            } => {
                let (_, _) = (severity_level, message);
                // TODO rely on Monitor
                println!("[LOG {severity_level}]: {message}");
                Ok(BrokerEventResult::Handled)
            }
            Event::CustomBuf { .. } => Ok(BrokerEventResult::Forward),
            //_ => Ok(BrokerEventResult::Forward),
        }
    }
}
