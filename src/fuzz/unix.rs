use super::{
    current_nanos, feedback_or, feedback_or_fast, havoc_mutations, load_tokens, mutate_args,
    ondisk, tokens_mutations, tuple_list, AsMutSlice, BytesInput, CachedOnDiskCorpus, Corpus,
    CrashFeedback, EventConfig, ForkserverExecutor, Fuzzer, FuzzerOptions, HasCorpus,
    HitcountsMapObserver, IndexesLenTimeMinimizerScheduler, LlmpRestartingEventManager,
    MaxMapFeedback, Merge, MultiMonitor, OnDiskCorpus, QueueScheduler, RandBytesGenerator, ShMem,
    ShMemProvider, StdMapObserver, StdRand, StdScheduledMutator, StdShMemProvider, StdState,
    TimeFeedback, TimeObserver, TimeoutFeedback, TimeoutForkserverExecutor,
};

#[cfg(feature = "tui")]
use super::tui::TuiMonitor;

use std::path::PathBuf;

use crate::components::{fuzzer::HeavyFuzzer, stages::CustomMutationalStage};
use crate::error::Error;
use crate::launcher::Launcher;

/// Fuzzer for unix-like systems
///
///
pub(super) fn fuzz(options: &FuzzerOptions) -> Result<(), Error> {
    // Component: Monitor
    #[cfg(feature = "tui")]
    let monitor = TuiMonitor::new("NnFuzz".to_string(), true);
    #[cfg(not(feature = "tui"))]
    let monitor = MultiMonitor::new(|s| println!("{s}"));

    // AFL++ compatible shmem provider
    let shmem_provider = StdShMemProvider::new()?;

    let mut run_client = |state: Option<_>, mut mgr: LlmpRestartingEventManager<_, _>, core_id| {
        let mut shmem_provider = StdShMemProvider::new()?;
        let mut shmem = shmem_provider.new_shmem(crate::MAP_SIZE).unwrap();
        // provide shmid for forkserver
        shmem.write_to_env("__AFL_SHM_ID").unwrap();

        // Component: Observers
        let edges_observer =
            HitcountsMapObserver::new(StdMapObserver::new("edges", shmem.as_mut_slice()));

        let time_observer = TimeObserver::new("time");

        // Component: Feedback
        // Rate input as interesting or not
        let mut feedback = feedback_or!(
            // max map feedback linked to edges observer
            MaxMapFeedback::new_tracking(&edges_observer, true, false),
            // time feedback (dont need feedback state)
            TimeFeedback::new_with_observer(&time_observer)
        );

        // Component: Objective
        // Rate input as fuzzing target (errors, SEGFAULTS ...)
        let mut objective = feedback_or_fast!(
            // crashes
            CrashFeedback::new(),
            // hangs
            TimeoutFeedback::new()
        );

        // Component: State
        let mut state = state.unwrap_or_else(|| {
            StdState::new(
                // RND
                StdRand::with_seed(options.seed.unwrap_or_else(current_nanos)),
                // Evol corpus
                CachedOnDiskCorpus::<BytesInput>::new(PathBuf::from("./corpus_discovered"), 64)
                    .unwrap(),
                // Solutions corpus
                OnDiskCorpus::new_save_meta(
                    options.output.clone(),
                    Some(ondisk::OnDiskMetadataFormat::JsonPretty),
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
        let mut fuzzer = HeavyFuzzer::new(scheduler, feedback, objective);

        // MUTATE arguments
        let mut harness_args = options.args.clone();
        if let Some(config) = options.core_args_config.as_ref() {
            mutate_args(harness_args.as_mut_slice(), config, core_id)?;
        }

        // Component: EXECUTOR
        let forkserver = ForkserverExecutor::builder()
            .program(options.executable.clone())
            .debug_child(options.debug_child)
            .shmem_provider(&mut shmem_provider)
            .arg_input_file(format!(".cur_input_{core_id}"))
            .parse_afl_cmdline(harness_args)
            .build(tuple_list!(time_observer, edges_observer))
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

        // MAINTAIN FUZZER STAGES
        // ======================
        let mutator =
            StdScheduledMutator::with_max_stack_pow(havoc_mutations().merge(tokens_mutations()), 6);

        let mut stages = tuple_list!(CustomMutationalStage::new(mutator));

        // RUUUN!
        fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
        Ok(())
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
        .spawn_nn_client(options.spawn_client)
        .remote_nn_port(options.client_port)
        .build()
        .launch()
}
