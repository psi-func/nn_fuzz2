use core::fmt::{self, Debug, Formatter};
use core::marker::PhantomData;

use std::fs::File;
#[cfg(windows)]
use std::process::Stdio;

#[cfg(windows)]
use libafl::bolts::{core_affinity::CoreId, os::startable_self};

use libafl::bolts::{core_affinity::Cores, shmem::ShMemProvider};
use libafl::events::{EventConfig, LlmpRestartingEventManager, ManagerKind, RestartingMgr};
use libafl::inputs::UsesInput;
use libafl::monitors::Monitor;
use libafl::state::{HasClientPerfMonitor, HasExecutions};
use libafl::Error;

use serde::de::DeserializeOwned;

use typed_builder::TypedBuilder;

use crate::llmp::NnRestartingMgr;

#[cfg(unix)]
use libafl::bolts::os::{dup2, fork, ForkResult};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

/// The (internal) `env` that indicates we're running as client.
const _AFL_LAUNCHER_CLIENT: &str = "AFL_LAUNCHER_CLIENT";

#[derive(TypedBuilder)]
pub struct Launcher<'a, CF, MT, S, SP>
where
    CF: FnOnce(Option<S>, LlmpRestartingEventManager<S, SP>, usize) -> Result<(), Error>,
    S::Input: 'a,
    MT: Monitor,
    SP: ShMemProvider + 'static,
    S: DeserializeOwned + UsesInput + 'a,
{
    /// The ShmemProvider to use
    shmem_provider: SP,
    /// The monitor instance to use
    monitor: MT,
    /// The configuration
    configuration: EventConfig,
    /// The 'main' function to run for each client forked. This probably shouldn't return
    #[builder(default, setter(strip_option))]
    run_client: Option<CF>,
    /// The broker port to use (or to attach to, in case [`Self::spawn_broker`] is `false`)
    #[builder(default = 1337_u16)]
    broker_port: u16,
    /// The list of cores to run on
    cores: &'a Cores,
    /// A file name to write all client output to
    #[builder(default = None)]
    stdout_file: Option<&'a str>,
    /// Should spawn nn client as separate llmp client
    #[builder(default = false)]
    spawn_nn_client: bool,
    /// The `port` of nn
    #[builder()]
    remote_nn_port: u16,
    /// If this launcher should spawn a new `broker` on `[Self::broker_port]` (default).
    /// The reason you may not want this is, if you already have a [`Launcher`]
    /// with a different configuration (for the same target) running on this machine.
    /// Then, clients launched by this [`Launcher`] can connect to the original `broker`.
    #[builder(default = true)]
    spawn_broker: bool,
    #[builder(setter(skip), default = PhantomData)]
    phantom_data: PhantomData<(&'a S, &'a SP)>,
}

impl<CF, MT, S, SP> Debug for Launcher<'_, CF, MT, S, SP>
where
    CF: FnOnce(Option<S>, LlmpRestartingEventManager<S, SP>, usize) -> Result<(), Error>,
    MT: Monitor + Clone,
    SP: ShMemProvider + 'static,
    S: DeserializeOwned + UsesInput,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Launcher")
            .field("configuration", &self.configuration)
            .field("broker_port", &self.broker_port)
            .field("core", &self.cores)
            .field("spawn_broker", &self.spawn_broker)
            .field("remote_broker_addr", &self.remote_nn_port)
            .field("stdout_file", &self.stdout_file)
            .finish_non_exhaustive()
    }
}

impl<'a, CF, MT, S, SP> Launcher<'a, CF, MT, S, SP>
where
    CF: FnOnce(Option<S>, LlmpRestartingEventManager<S, SP>, usize) -> Result<(), Error>,
    MT: Monitor + Clone,
    S: DeserializeOwned + UsesInput + HasExecutions + HasClientPerfMonitor,
    SP: ShMemProvider + 'static,
{
    /// Launch the broker and the clients and fuzz
    #[cfg(unix)]
    #[allow(clippy::similar_names)]
    pub fn launch(&mut self) -> Result<(), Error> {
        use libafl::bolts::core_affinity::get_core_ids;

        if self.run_client.is_none() {
            return Err(Error::illegal_argument(
                "No client callback provided".to_string(),
            ));
        }

        let core_ids = get_core_ids().unwrap();
        let num_cores = core_ids.len();
        let mut handles = vec![];

        println!("spawning on cores: {:?}", self.cores);

        let stdout_file = self
            .stdout_file
            .map(|filename| File::create(filename).unwrap());

        let debug_output = std::env::var("LIBAFL_DEBUG_OUTPUT").is_ok();

        // Spawn clients
        let mut index = 0_u64;
        for (id, bind_to) in core_ids.iter().enumerate().take(num_cores) {
            if self.cores.ids.iter().any(|&x| x == id.into()) {
                index += 1;
                self.shmem_provider.pre_fork()?;
                match unsafe { fork() }? {
                    ForkResult::Parent(child) => {
                        self.shmem_provider.post_fork(false)?;
                        handles.push(child.pid);

                        println!("child spawned and bound to core {id}");
                    }
                    ForkResult::Child => {
                        println!("{:?} PostFork", unsafe { libc::getpid() });
                        self.shmem_provider.post_fork(true)?;

                        std::thread::sleep(std::time::Duration::from_millis(index * 100));

                        if !debug_output {
                            if let Some(file) = stdout_file {
                                dup2(file.as_raw_fd(), libc::STDOUT_FILENO)?;
                                dup2(file.as_raw_fd(), libc::STDERR_FILENO)?;
                            }
                        }

                        // Fuzzer client. keeps retrying the connection to broker till the broker starts
                        let (state, mgr) = RestartingMgr::<MT, S, SP>::builder()
                            .shmem_provider(self.shmem_provider.clone())
                            .broker_port(self.broker_port)
                            .kind(ManagerKind::Client {
                                cpu_core: Some(*bind_to),
                            })
                            .configuration(self.configuration)
                            .build()
                            .launch()?;

                        return (self.run_client.take().unwrap())(state, mgr, bind_to.0);
                    }
                };
            }
        }

        if self.spawn_broker {
            println!("I am broker!!.");

            // TODO: change manager
            NnRestartingMgr::<MT, S, SP>::builder()
                .shmem_provider(self.shmem_provider.clone())
                .monitor(Some(self.monitor.clone()))
                .broker_port(self.broker_port)
                .configuration(self.configuration)
                .spawn_nn_client(self.spawn_nn_client)
                .remote_nn_port(self.remote_nn_port)
                .build()
                .launch()?;

            // Broker exited. kill all clients.
            for handle in &handles {
                unsafe {
                    libc::kill(*handle, libc::SIGINT);
                }
            }
        } else {
            for handle in &handles {
                let mut status = 0;
                println!("Not spawning broker (spawn_broker is false). Waiting for fuzzer children to exit...");
                unsafe {
                    libc::waitpid(*handle, &mut status, 0);
                    if status != 0 {
                        println!("Client with pid {handle} exited with status {status}");
                    }
                }
            }
        }

        Ok(())
    }

    /// Launch the broker and the clients and fuzz
    #[cfg(windows)]
    #[allow(unused_mut, clippy::match_wild_err_arm)]
    pub fn launch(&mut self) -> Result<(), Error> {
        let is_client = std::env::var(_AFL_LAUNCHER_CLIENT);

        let mut handles = match is_client {
            Ok(core_conf) => {
                let core_id = core_conf.parse()?;

                //todo: silence stdout and stderr for clients

                // the actual client. do the fuzzing
                let (state, mgr) = RestartingMgr::<MT, S, SP>::builder()
                    .shmem_provider(self.shmem_provider.clone())
                    .broker_port(self.broker_port)
                    .kind(ManagerKind::Client {
                        cpu_core: Some(CoreId { id: core_id }),
                    })
                    .configuration(self.configuration)
                    .build()
                    .launch()?;

                return (self.run_client.take().unwrap())(state, mgr, core_id);
            }
            Err(std::env::VarError::NotPresent) => {
                // I am a broker
                // before going to the broker loop, spawn n clients

                #[cfg(windows)]
                if self.stdout_file.is_some() {
                    println!("Child process file stdio is not supported on Windows yet. Dumping to stdout instead...");
                }

                let core_ids = core_affinity::get_core_ids().unwrap();
                let num_cores = core_ids.len();
                let mut handles = vec![];

                println!("spawning on cores: {:?}", self.cores);

                //spawn clients
                for (id, _) in core_ids.iter().enumerate().take(num_cores) {
                    if self.cores.ids.iter().any(|&x| x == id.into()) {
                        let stdio = if self.stdout_file.is_some() {
                            Stdio::inherit()
                        } else {
                            Stdio::null()
                        };

                        std::env::set_var(_AFL_LAUNCHER_CLIENT, id.to_string());
                        let child = startable_self()?.stdout(stdio).spawn()?;
                        handles.push(child);
                    }
                }

                handles
            }
            Err(_) => panic!("Env variables are broken, received non-unicode!"),
        };

        if self.spawn_broker {
            #[cfg(feature = "std")]
            println!("I am broker!!.");

            RestartingMgr::<MT, S, SP>::builder()
                .shmem_provider(self.shmem_provider.clone())
                .monitor(Some(self.monitor.clone()))
                .broker_port(self.broker_port)
                .kind(ManagerKind::Broker)
                .remote_broker_addr(self.remote_broker_addr)
                .configuration(self.configuration)
                .build()
                .launch()?;

            //broker exited. kill all clients.
            for handle in &mut handles {
                handle.kill()?;
            }
        } else {
            println!("Not spawning broker (spawn_broker is false). Waiting for fuzzer children to exit...");
            for handle in &mut handles {
                let ecode = handle.wait()?;
                if !ecode.success() {
                    println!("Client with handle {:?} exited with {:?}", handle, ecode);
                }
            }
        }

        Ok(())
    }
}
