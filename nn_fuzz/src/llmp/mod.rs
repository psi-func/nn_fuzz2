pub mod extention;

use core::marker::PhantomData;

use libafl::bolts::shmem::ShMemProvider;
use libafl::events::EventConfig;
use libafl::inputs::UsesInput;
use libafl::monitors::Monitor;
use libafl::state::{HasClientPerfMonitor, HasExecutions};
use libafl::Error;

use serde::de::DeserializeOwned;
use typed_builder::TypedBuilder;

use self::extention::{LlmpNnEventBroker, RestartingNnEventManager};

/// The llmp connection from the actual fuzzer to the process supervising it
const _ENV_FUZZER_SENDER: &str = "_AFL_ENV_FUZZER_SENDER";
const _ENV_FUZZER_RECEIVER: &str = "_AFL_ENV_FUZZER_RECEIVER";
/// The llmp (2 way) connection from a fuzzer to the broker (broadcasting all other fuzzer messages)
const _ENV_FUZZER_BROKER_CLIENT_INITIAL: &str = "_AFL_ENV_FUZZER_BROKER_CLIENT";

#[derive(TypedBuilder, Debug)]
pub struct NnRestartingMgr<MT, S, SP>
where
    S: UsesInput + DeserializeOwned,
    SP: ShMemProvider + 'static,
    MT: Monitor,
{
    /// The shared memory provider to use for the broker or client spawned by the restarting
    /// manager.
    shmem_provider: SP,
    /// The configuration
    configuration: EventConfig,
    /// The monitor to use
    #[builder(default = None)]
    monitor: Option<MT>,
    /// The broker port to use
    #[builder(default = 1337_u16)]
    broker_port: u16,
    /// Spawn nn server
    spawn_nn_client: bool,
    /// The neural network port to use
    #[builder(default = 7878_u16)]
    remote_nn_port: u16,

    #[builder(setter(skip), default = PhantomData)]
    phantom_data: PhantomData<S>,
}

impl<MT, S, SP> NnRestartingMgr<MT, S, SP>
where
    S: UsesInput + HasExecutions + HasClientPerfMonitor + DeserializeOwned,
    SP: ShMemProvider,
    MT: Monitor,
{
    /// Launch the restarting manager
    pub fn launch(&mut self) -> Result<(Option<S>, RestartingNnEventManager<S, SP>), Error> {
        // We start ourself as child process to actually fuzz
        let broker_things = |mut broker: LlmpNnEventBroker<S::Input, MT, SP>, remote_nn_port| {
            if let Some(nn_port) = remote_nn_port {
                println!("B2b: Connecting to {:?}", &nn_port);
                broker.spawn_client(nn_port);
            };

            broker.broker_loop()
        };

        let event_broker = LlmpNnEventBroker::<S::Input, MT, SP>::new_on_port(
            self.shmem_provider.clone(),
            self.monitor.take().unwrap(),
            self.broker_port,
        )?;

        broker_things(
            event_broker,
            if self.spawn_nn_client {
                Some(self.remote_nn_port)
            } else {
                None
            },
        )?;

        Err(Error::shutting_down())
    }
}
