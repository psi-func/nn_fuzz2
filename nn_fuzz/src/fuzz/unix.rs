use super::{
    current_nanos, feedback_or, feedback_or_fast, feedback_and, havoc_mutations, load_tokens, mutate_args,
    ondisk, tokens_mutations, tuple_list, AsMutSlice, AsanBacktraceObserver, BytesInput,
    CachedOnDiskCorpus, Corpus, CrashFeedback, EventConfig, ForkserverExecutor, Fuzzer,
    FuzzerOptions, HasCorpus, HitcountsMapObserver, IndexesLenTimeMinimizerScheduler,
    LlmpRestartingEventManager, MaxMapFeedback, Merge, NewHashFeedback, OnDiskCorpus,
    QueueScheduler, RandBytesGenerator, ShMem, ShMemProvider, StdMapObserver, StdRand,
    StdScheduledMutator, StdShMemProvider, StdState, TimeFeedback, TimeObserver, TimeoutFeedback,
    TimeoutForkserverExecutor, StdMutationalStage, CalibrationStage
};

#[cfg(not(feature = "observer_feedback"))]
use super::StdFuzzer;

#[cfg(feature = "observer_feedback")]
use crate::components::fuzzer::HeavyFuzzer;

#[cfg(feature = "net_monitor")]
use super::monitors::PrometheusMonitor;
#[cfg(not(feature = "net_monitor"))]
use super::monitors::MultiMonitor;

use crate::error::Error;
use crate::launcher::Launcher;

/// Fuzzer for unix-like systems
///
///
#[allow(clippy::too_many_lines)]
pub(super) fn fuzz(options: &FuzzerOptions) -> Result<(), Error> {
    // Component: Monitor
    #[cfg(feature = "net_monitor")]
    let monitor = PrometheusMonitor::new("127.0.0.1:8080".to_string(), |s| log::info!("{s}"));
    #[cfg(not(feature = "net_monitor"))]
    let monitor = MultiMonitor::new(|s| println!("{s}"));

    // AFL++ compatible shmem provider
    let shmem_provider = StdShMemProvider::new()?;

    let mut run_client =
        |state: Option<_>, mut mgr: LlmpRestartingEventManager<_, _>, core_id: usize| {
            let mut shmem_provider = StdShMemProvider::new()?;
            let mut shmem = shmem_provider.new_shmem(crate::MAP_SIZE).unwrap();
            // provide shmid for forkserver
            shmem.write_to_env("__AFL_SHM_ID").unwrap();

            // Component: Observers
            let edges_observer = HitcountsMapObserver::new(unsafe {
                StdMapObserver::new("edges", shmem.as_mut_slice())
            });

            if options.backtrace {
                let bt_observer = AsanBacktraceObserver::default();

                // Component: Feedback
                // Rate input as interesting or not
                let mut feedback = 
                    // max map feedback linked to edges observer
                    MaxMapFeedback::tracking(&edges_observer, true, false);
                

                // MAINTAIN FUZZER STAGES
                // ======================
                let mutator = StdScheduledMutator::with_max_stack_pow(
                    havoc_mutations().merge(tokens_mutations()),
                    6,
                );

                let mut stages = tuple_list!(CalibrationStage::new(&feedback), StdMutationalStage::new(mutator));

                let mut objective = feedback_and!(
                    CrashFeedback::new(),
                    // backtrace hash observer
                    NewHashFeedback::new(&bt_observer)
                );

                // Component: State
                let mut state = state.unwrap_or_else(|| {
                    StdState::new(
                        // RND
                        StdRand::with_seed(match &options.seed.vals {
                            Some(vals) => {
                                let (_, &seed) = options
                            .cores
                            .ids
                            .iter()
                            .zip(vals.iter())
                            .find(|(&core, _)| core == core_id.into())
                            .unwrap_or_else(|| {
                                panic!("Cannot set seed to [Core {core_id}] from list {vals:?}");
                            });
                                println!("[Core {core_id}] setup seed: {seed}");
                                seed
                            }
                            None => {
                                println!("[Core {core_id}] setup seed: auto");
                                current_nanos()
                            }
                        }),
                        // Evol corpus
                        CachedOnDiskCorpus::<BytesInput>::new(
                            format!("{}_{}", options.queue.to_str().unwrap(), core_id),
                            64,
                        )
                        .unwrap(),
                        // Solutions corpus
                        OnDiskCorpus::with_meta_format(
                            options.output.clone(),
                            ondisk::OnDiskMetadataFormat::JsonPretty,
                        )
                        .unwrap(),
                        // ----------
                        &mut feedback,
                        &mut objective,
                    )
                    .unwrap()
                });

                println!("start fuzzer...");

                // LOAD TOKENS
                load_tokens(options.tokens.as_slice(), &mut state, &mut mgr)?;

                // Component: Scheduler
                let scheduler = IndexesLenTimeMinimizerScheduler::new(QueueScheduler::new());

                // Component: Real Fuzzer
                #[cfg(feature = "observer_feedback")]
                let mut fuzzer = HeavyFuzzer::new(scheduler, feedback, objective);

                #[cfg(not(feature = "observer_feedback"))]
                let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

                // MUTATE arguments
                let mut harness_args = options.args.clone();
                if let Some(config) = options.core_args_config.as_ref() {
                    mutate_args(harness_args.as_mut_slice(), config, core_id)?;
                }

                // Component: EXECUTOR
                let mut executor = ForkserverExecutor::builder()
                    .program(options.executable.clone())
                    .envs(options.envs.clone())
                    .debug_child(options.debug_child)
                    .shmem_provider(&mut shmem_provider)
                    //.arg_input_file(format!(".cur_input_{core_id}"))
                    .parse_afl_cmdline(harness_args)
                    .build_dynamic_map(edges_observer, tuple_list!(bt_observer,))
                    .expect("Failed to create executor.");

                // LOAD or GENERATE initial seeds
                // ==============================
                if state.corpus().count() < 1 {
                    if let Some(ref inputs) = options.input {
                        state
                            .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, inputs)
                            .unwrap_or_else(|_| {
                                panic!("Failed to load initial corpus at {:?}", &options.input);
                            });
                        println!("We imported {} inputs from disk.", state.corpus().count());
                    } else {
                        let mut generator = RandBytesGenerator::new(options.input_max_length);
                        state
                            .generate_initial_inputs(
                                &mut fuzzer,
                                &mut executor,
                                &mut generator,
                                &mut mgr,
                                options.generate_count,
                            )
                            .unwrap_or_else(|_| panic!("Failed to generate the initial corpus."));
                        println!(
                            "Generated {} elements with interesting coverage",
                            state.corpus().count()
                        );
                    }
                }

                // RUUUN!
                fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;

                Ok(())
            } else {
                let time_observer = TimeObserver::new("time");

                let map_feedback = MaxMapFeedback::tracking(&edges_observer, true, false);
                
                // MAINTAIN FUZZER STAGES
                // ======================
                let mutator = StdScheduledMutator::with_max_stack_pow(
                    havoc_mutations().merge(tokens_mutations()),
                    6,
                );

                #[cfg(not(feature="power_sched"))]
                let mut stages = tuple_list!(StdMutationalStage::new(mutator));
                #[cfg(feature="power_sched")]
                let mut stages = tuple_list!(CalibrationStage::new(&feedback), StdMutationalStage::new(mutator));
        
                // Component: Feedback
                // Rate input as interesting or not
                let mut feedback = feedback_or!(
                    // max map feedback linked to edges observer
                    map_feedback,
                    // time feedback (dont need feedback state)
                    TimeFeedback::with_observer(&time_observer)
                );

                // Component: Objective
                // Rate input as fuzzing target (errors, SEGFAULTS ...)
                let mut objective = feedback_or_fast!(
                    // save time metadata
                    TimeFeedback::with_observer(&time_observer), 
                    // crashes
                    CrashFeedback::new(),
                    // hangs
                    TimeoutFeedback::new()
                );

                // Component: State
                let mut state = state.unwrap_or_else(|| {
                    StdState::new(
                        // RND
                        StdRand::with_seed(match &options.seed.vals {
                            Some(vals) => {
                                let (_, &seed) = options
                        .cores
                        .ids
                        .iter()
                        .zip(vals.iter())
                        .find(|(&core, _)| core == core_id.into())
                        .unwrap_or_else(|| {
                            panic!("Cannot set seed to [Core {core_id}] from list {vals:?}");
                        });
                                println!("[Core {core_id}] setup seed: {seed}");
                                seed
                            }
                            None => {
                                println!("[Core {core_id}] setup seed: auto");
                                current_nanos()
                            }
                        }),
                        // Evol corpus
                        CachedOnDiskCorpus::<BytesInput>::new(
                            format!("{}_{}", options.queue.to_str().unwrap(), core_id),
                            64,
                        )
                        .unwrap(),
                        // Solutions corpus
                        OnDiskCorpus::with_meta_format(
                            options.output.clone(),
                            ondisk::OnDiskMetadataFormat::JsonPretty,
                        )
                        .unwrap(),
                        // ----------
                        &mut feedback,
                        &mut objective,
                    )
                    .unwrap()
                });

                println!("start fuzzer...");

                // LOAD TOKENS
                load_tokens(options.tokens.as_slice(), &mut state, &mut mgr)?;

                // Component: Scheduler
                let scheduler = IndexesLenTimeMinimizerScheduler::new(QueueScheduler::new());

                // Component: Real Fuzzer
                #[cfg(feature = "observer_feedback")]
                let mut fuzzer = HeavyFuzzer::new(scheduler, feedback, objective);

                #[cfg(not(feature = "observer_feedback"))]
                let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

                // MUTATE arguments
                let mut harness_args = options.args.clone();
                if let Some(config) = options.core_args_config.as_ref() {
                    mutate_args(harness_args.as_mut_slice(), config, core_id)?;
                }

                // Component: EXECUTOR
                let forkserver = ForkserverExecutor::builder()
                    .program(options.executable.clone())
                    .envs(options.envs.clone())
                    .debug_child(options.debug_child)
                    .shmem_provider(&mut shmem_provider)
                    //.arg_input_file(format!(".cur_input_{core_id}"))
                    .parse_afl_cmdline(harness_args)
                    .build_dynamic_map(edges_observer, tuple_list!(time_observer))
                    .unwrap();

                let mut executor = TimeoutForkserverExecutor::new(forkserver, options.timeout)
                    .expect("Failed to create executor.");

                // LOAD or GENERATE initial seeds
                // ==============================
                if state.corpus().count() < 1 {
                    if let Some(ref inputs) = options.input {
                        state
                            .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, inputs)
                            .unwrap_or_else(|_| {
                                panic!("Failed to load initial corpus at {:?}", &options.input);
                            });
                        println!("We imported {} inputs from disk.", state.corpus().count());
                    } else {
                        let mut generator = RandBytesGenerator::new(options.input_max_length);
                        state
                            .generate_initial_inputs(
                                &mut fuzzer,
                                &mut executor,
                                &mut generator,
                                &mut mgr,
                                options.generate_count,
                            )
                            .unwrap_or_else(|_| panic!("Failed to generate the initial corpus."));
                        println!(
                            "Generated {} elements with interesting coverage",
                            state.corpus().count()
                        );
                    }
                }

                // RUUUN!
                fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
                Ok(())
            }
        }; // run_client closure

    // LLMP init
    // =========
    Launcher::builder()
        .configuration(EventConfig::AlwaysUnique)
        .shmem_provider(shmem_provider)
        .monitor(monitor)
        .run_client(&mut run_client)
        .cores(&options.cores)
        .broker_port(options.broker_port)
        .stdout_file(options.stdout.as_deref())
        .spawn_broker(!options.no_broker)
        .spawn_nn_client(options.spawn_client)
        .remote_nn_port(options.client_port)
        .build()
        .launch()
}
