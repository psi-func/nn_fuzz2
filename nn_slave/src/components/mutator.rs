#![allow(dead_code)]
#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]

use crate::{
    cli::SlaveOptions,
    nn::mutatios::{rl_mutations, NnMutator, RlMutationTuple},
    nn::{NeuralNetwork, TaskCompletion},
};

use core::marker::PhantomData;

use libafl::{
    bolts::rands::Rand,
    corpus::Corpus,
    corpus::CorpusId,
    events::{Event, EventFirer},
    executors::HasObservers,
    fuzzer::ExecutesInput,
    inputs::UsesInput,
    mark_feature_time,
    monitors::UserStats,
    mutators::Mutator,
    observers::ObserversTuple,
    prelude::{
        AsSlice, HasBytesVec, HitcountsMapObserver, MapObserver, MutationResult, StdMapObserver,
    },
    stages::Stage,
    start_timer,
    state::{
        HasClientPerfMonitor, HasCorpus, HasExecutions, HasMaxSize, HasMetadata, HasRand,
        HasSolutions, UsesState,
    },
    Error, ExecutionProcessor, SerdeAny,
};

use serde::{Deserialize, Serialize};

pub trait MutationalStage<E, EM, M, Z, OT>: Stage<E, EM, Z>
where
    E: UsesState<State = Self::State> + HasObservers<Observers = OT>,
    M: Mutator<<Self as UsesInput>::Input, Self::State>,
    EM: UsesState<State = Self::State> + EventFirer,
    OT: ObserversTuple<Self::State> + Serialize,
    Z: ExecutesInput<E, EM, State = Self::State> + ExecutionProcessor<OT>,
    Self::State: HasClientPerfMonitor + HasCorpus + HasSolutions + HasExecutions,
{
    /// The mutator registered for stage
    fn mutator(&self) -> &M;

    /// The mutator registered for this stage (mutable)
    fn mutator_mut(&mut self) -> &mut M;

    /// Gets the number of iteration this mutator should run for.
    fn iterations(&self, state: &mut Z::State, corpus_idx: CorpusId) -> Result<usize, Error>;

    /// Runs stage for testcase
    fn perform_mutational(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut Z::State,
        manager: &mut EM,
        corpus_idx: CorpusId,
    ) -> Result<(), Error>;
}

#[derive(Serialize, Deserialize, SerdeAny, Debug, Clone)]
pub struct MutationMeta {
    depth: u64,
}

impl MutationMeta {
    pub fn new() -> Self {
        Self { depth: 0 }
    }

    pub fn depth(&self) -> &u64 {
        &self.depth
    }

    pub fn depth_mut(&mut self) -> &mut u64 {
        &mut self.depth
    }
}

static DEFAULT_MUTATIONAL_MAX_ITERATIONS: u64 = 128;

#[derive(Debug)]
pub struct CustomMutationalStage<E, EM, M, Z, OT>
where
    Z: UsesState,
    Z::State: HasRand + HasMaxSize,
    Z::Input: HasBytesVec,
{
    neural_network: NeuralNetwork<Z::Input>,
    counter: u32,
    blocker: bool,
    nn_mutator: NnMutator<Z::Input, RlMutationTuple, Z::State>,
    mutator: M,
    max_depth: u64,
    phantom: PhantomData<(E, EM, Z, OT)>,
}

impl<E, EM, M, Z, OT> CustomMutationalStage<E, EM, M, Z, OT>
where
    Z: UsesState,
    Z::State: HasRand + HasMaxSize,
    Z::Input: HasBytesVec + std::marker::Send + 'static,
{
    pub fn new(mutator: M, options: &SlaveOptions) -> Self {
        Self {
            neural_network: NeuralNetwork::new(options),
            counter: 0,
            blocker: false,
            nn_mutator: NnMutator::new(rl_mutations()),
            mutator,
            max_depth: 0,
            phantom: PhantomData,
        }
    }
}

impl<E, EM, M, Z, OT> UsesState for CustomMutationalStage<E, EM, M, Z, OT>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    Z: ExecutesInput<E, EM>,
    Z::Input: HasBytesVec,
    Z::State:
        HasClientPerfMonitor + HasCorpus + HasSolutions + HasExecutions + HasRand + HasMaxSize,
{
    type State = Z::State;
}

impl<E, EM, M, Z, OT> Stage<E, EM, Z> for CustomMutationalStage<E, EM, M, Z, OT>
where
    E: UsesState<State = Z::State> + HasObservers<Observers = OT>,
    M: Mutator<Z::Input, Z::State>,
    EM: UsesState<State = Z::State> + EventFirer,
    OT: ObserversTuple<Z::State> + Serialize,
    Z: ExecutesInput<E, EM> + ExecutionProcessor<OT>,
    Z::State: HasClientPerfMonitor
        + HasCorpus
        + HasSolutions
        + HasExecutions
        + HasRand
        + HasMaxSize
        + HasMetadata,
    Z::Input: HasBytesVec + std::marker::Send + 'static,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut Self::State,
        manager: &mut EM,
        corpus_idx: CorpusId,
    ) -> Result<(), Error> {
        self.perform_mutational(fuzzer, executor, state, manager, corpus_idx)
    }
}

impl<E, EM, M, Z, OT> MutationalStage<E, EM, M, Z, OT> for CustomMutationalStage<E, EM, M, Z, OT>
where
    E: UsesState<State = Z::State> + HasObservers<Observers = OT>,
    M: Mutator<Z::Input, Z::State>,
    EM: UsesState<State = Z::State> + EventFirer,
    OT: ObserversTuple<Z::State> + Serialize,
    Z: ExecutesInput<E, EM> + ExecutionProcessor<OT>,
    Z::State: HasClientPerfMonitor
        + HasCorpus
        + HasSolutions
        + HasExecutions
        + HasRand
        + HasMetadata
        + HasMaxSize,
    Z::Input: HasBytesVec + std::marker::Send + 'static,
{
    fn mutator(&self) -> &M {
        &self.mutator
    }

    fn mutator_mut(&mut self) -> &mut M {
        &mut self.mutator
    }

    fn iterations(&self, state: &mut <Z>::State, _corpus_idx: CorpusId) -> Result<usize, Error> {
        Ok(1 + state.rand_mut().below(DEFAULT_MUTATIONAL_MAX_ITERATIONS) as usize)
    }

    #[allow(clippy::too_many_lines)]
    fn perform_mutational(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut <Z>::State,
        manager: &mut EM,
        corpus_idx: CorpusId,
    ) -> Result<(), Error> {
        if !self.blocker {
            let mut testcase = state
                .corpus()
                .get(corpus_idx)?
                .borrow_mut();
            state.corpus().load_input_into(&mut testcase)?;
            let input = testcase.input().as_ref().unwrap().clone();
            self.neural_network.predict(corpus_idx, input)?;
            self.blocker = true;
        }

        match self.neural_network.nn_responce() {
            None => {}
            Some(TaskCompletion::NnDropped) => {
                println!("Neural network dropped, renew connection");
                self.blocker = false;
            }
            Some(TaskCompletion::Prediction { id, heatmap }) => {
                self.blocker = false;
                // mutations for hotbytes

                *self.nn_mutator.hotbytes_mut() = heatmap;

                let num = self.iterations(state, id)?;
                let mut diffs: Vec<u32> = Vec::with_capacity(num);

                let input =  {
                    let mut testcase = state.corpus().get(id)?.borrow_mut();
                    state.corpus().load_input_into(&mut testcase)?;
                    testcase.input().as_ref().unwrap().clone()
                };

                let _exit_kind = fuzzer.execute_input(state, executor, manager, &input)?;
                let observers = executor.observers();
                let edges = observers
                    .match_name::<HitcountsMapObserver<StdMapObserver<u8, false>>>("edges")
                    .unwrap_or_else(|| panic!("Incorrect observer name: MUST be edges"));
                let original_map = edges.to_vec();

                let mut skipped_counter = 0;

                for i in 0..num {
                    let mut input = input.clone();
                    if let MutationResult::Skipped =
                        self.nn_mutator.mutate(state, &mut input, i as i32)?
                    {
                        skipped_counter += 1;
                    }

                    // execute
                    let exit_kind = fuzzer.execute_input(state, executor, manager, &input)?;
                    let observers = executor.observers();
                    let edges = observers
                        .match_name::<HitcountsMapObserver<StdMapObserver<u8, false>>>("edges")
                        .unwrap_or_else(|| panic!("Incorrect observer name: MUST be edges"));

                    diffs.push(
                        original_map
                            .iter()
                            .zip(edges.as_slice().iter())
                            .filter_map(|(&orig, &i)| {
                                if orig < i {
                                    Some(u32::from(i - orig))
                                } else {
                                    None
                                }
                            })
                            .sum(),
                    );

                    let (_, _corpus_idx) = fuzzer
                        .process_execution(state, manager, input, observers, &exit_kind, true)?;
                }

                let length = diffs.len();
                #[allow(clippy::cast_precision_loss)]
                let reward =
                    diffs.iter().fold(0_u64, |sum, &i| sum + u64::from(i)) as f64 / length as f64;
                self.neural_network.reward(reward)?;

                println!("Send reward to neural network {reward}, mutations: {num}, skipped: {skipped_counter}, diffs: {diffs:?}");
                return Ok(());
            }
        }

        let num = self.iterations(state, corpus_idx)?;

        for i in 0..num {
            start_timer!(state);

            let exist_depth;
            let mut input = 
            {
                let mut testcase = state.corpus().get(corpus_idx)?.borrow_mut();
                exist_depth = if testcase.has_metadata::<MutationMeta>() {
                    *testcase.metadata::<MutationMeta>().unwrap().depth()
                } else {
                    testcase.add_metadata::<MutationMeta>(MutationMeta { depth: 1 });
                    1
                };
                state.corpus().load_input_into(&mut testcase)?;
                testcase.input().as_ref().unwrap().clone()
            };
            mark_feature_time!(state, PerfFeature::GetInputFromCorpus);

            start_timer!(state);
            self.mutator_mut().mutate(state, &mut input, i as i32)?;
            mark_feature_time!(state, PerfFeature::Mutate);

            let exit_kind = fuzzer.execute_input(state, executor, manager, &input)?;
            let observers = executor.observers();

            let (_, corpus_idx) =
                fuzzer.process_execution(state, manager, input, observers, &exit_kind, true)?;

            {
                if let Some(idx) = corpus_idx {
                    let depth = {
                        let mut testcase = state.corpus().get(idx)?.borrow_mut();

                        testcase.add_metadata::<MutationMeta>(MutationMeta {
                            depth: exist_depth + 1,
                        });
                        exist_depth + 1
                    };

                    if self.max_depth < depth {
                        self.max_depth = depth;

                        manager.fire(
                            state,
                            Event::UpdateUserStats {
                                name: "max_depth".to_string(),
                                value: UserStats::Number(depth),
                                phantom: PhantomData,
                            },
                        )?;
                    }
                }
            }

            start_timer!(state);
            self.mutator_mut().post_exec(state, i as i32, corpus_idx)?;
            mark_feature_time!(state, PerfFeature::MutatePostExec);
        }
        Ok(())
    }
}
