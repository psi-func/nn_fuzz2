#![allow(dead_code)]
#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]

use core::marker::PhantomData;

use libafl::{
    bolts::rands::Rand,
    corpus::Corpus,
    corpus::CorpusId,
    events::{Event, EventFirer},
    executors::HasObservers,
    fuzzer::ExecutesInput,
    mark_feature_time,
    monitors::UserStats,
    mutators::Mutator,
    observers::ObserversTuple,
    stages::Stage,
    start_timer,
    state::{
        HasClientPerfMonitor, HasCorpus, HasExecutions, HasMetadata, HasRand, HasSolutions,
        UsesState,
    },
    Error, ExecutionProcessor, SerdeAny,
};

use serde::{Deserialize, Serialize};

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

pub static DEFAULT_MUTATIONAL_MAX_ITERATIONS: u64 = 128;

#[derive(Clone, Debug)]
pub struct CustomMutationalStage<E, EM, M, Z, OT> {
    mutator: M,
    max_depth: u64,
    phantom: PhantomData<(E, EM, Z, OT)>,
}

impl<E, EM, M, Z, OT> CustomMutationalStage<E, EM, M, Z, OT> {
    pub fn new(mutator: M) -> Self {
        Self {
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
    Z::State: HasClientPerfMonitor + HasCorpus + HasSolutions + HasExecutions + HasRand,
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
    Z::State:
        HasClientPerfMonitor + HasCorpus + HasSolutions + HasExecutions + HasRand + HasMetadata,
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

impl<E, EM, M, Z, OT> CustomMutationalStage<E, EM, M, Z, OT>
where
    E: UsesState<State = Z::State> + HasObservers<Observers = OT>,
    M: Mutator<Z::Input, Z::State>,
    EM: UsesState<State = Z::State> + EventFirer,
    OT: ObserversTuple<Z::State> + Serialize,
    Z: ExecutesInput<E, EM> + ExecutionProcessor<OT>,
    Z::State:
        HasClientPerfMonitor + HasCorpus + HasSolutions + HasExecutions + HasRand + HasMetadata,
{
    fn mutator(&self) -> &M {
        &self.mutator
    }

    fn mutator_mut(&mut self) -> &mut M {
        &mut self.mutator
    }

    fn iterations(state: &mut <Z>::State, _corpus_idx: CorpusId) -> usize {
        1 + state.rand_mut().below(DEFAULT_MUTATIONAL_MAX_ITERATIONS) as usize
    }

    fn perform_mutational(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut <Z>::State,
        manager: &mut EM,
        corpus_idx: CorpusId,
    ) -> Result<(), Error> {
        let num = Self::iterations(state, corpus_idx);

        for i in 0..num {
            start_timer!(state);

            let exist_depth;
            let mut input = {
                let mut testcase = state.corpus().get(corpus_idx)?.borrow_mut();
                exist_depth = if testcase.has_metadata::<MutationMeta>() {
                    *testcase.metadata::<MutationMeta>().unwrap().depth()
                } else {
                    testcase.add_metadata::<MutationMeta>(MutationMeta { depth: 1 });
                    1
                };
                // input = Z::Input
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
