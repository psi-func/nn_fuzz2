use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use libafl::prelude::*;

use crate::cli::FuzzerOptions;

#[cfg_attr(target_family = "windows", path = "win.rs")]
#[cfg_attr(target_family = "unix", path = "unix.rs")]
mod detail;

///
/// # Errors
///
/// - ``Error::ShuttingDown`` when fuzzer stops normally
///
pub fn fuzz(options: &FuzzerOptions) -> Result<(), Error> {
    check_options(options)?;
    detail::fuzz(options)
}

fn check_options(options: &FuzzerOptions) -> Result<(), Error> {
    if let Some(ref vals) = options.seed.vals {
        if options.cores.ids.len() > vals.len() {
            return Err(Error::illegal_argument(format!("Invalid seed size! Please, provide {} unique seeds", options.cores.ids.len())));
        }
        // other checks
    }
    Ok(())
}

fn load_tokens<EM, S>(dicts: &[PathBuf], state: &mut S, mgr: &mut EM) -> Result<(), Error>
where
    EM: EventFirer<State = S>,
    S: HasMetadata + UsesInput,
{
    if state.metadata::<Tokens>().is_err() && !dicts.is_empty() {
        let mut tokens = Tokens::new();
        // load tokens
        tokens = match tokens.add_from_files(dicts) {
            Ok(tokens) => {
                mgr.fire(
                    state,
                    Event::Log {
                        severity_level: LogSeverity::Debug,
                        message: format!("Loaded tokens {} from {dicts:?}", tokens.len()),
                        phantom: PhantomData::<S::Input>,
                    },
                )?;
                tokens
            }
            Err(e) => {
                return Err(e);
            }
        };
        state.add_metadata(tokens);
    };
    Ok(())
}

fn mutate_args(args: &mut [String], config: &Path, core_id: usize) -> Result<(), Error> {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::fs;

    #[derive(Copy, Clone, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    enum Mutation {
        Increment,
    }

    fn increment(arg: &mut String, core_id: usize) -> Result<(), Error> {
        match arg.parse::<usize>() {
            Ok(value) => {
                *arg = format!("{}", value + core_id);
                Ok(())
            }
            Err(e) => Err(Error::illegal_argument(e.to_string())),
        }
    }

    impl Mutation {
        fn mutate(self, arg: &mut String, core_id: usize) -> Result<(), Error> {
            match self {
                Mutation::Increment => increment(arg, core_id),
            }
        }
    }

    #[allow(clippy::zero_sized_map_values)]
    let works: HashMap<String, Mutation> = fs::read_to_string(config).map(|text| {
        serde_json::from_str(&text).unwrap_or_else(|_| {
            panic!("Invalid configure file! {config:?}");
        })
    })?;

    {
        let mut mutate_next: Option<Mutation> = None;
        for arg in args.iter_mut() {
            if let Some(mutation) = mutate_next.take() {
                mutation.mutate(arg, core_id)?;
            }
            if let Some(mutation) = works.get(arg) {
                mutate_next = Some(*mutation);
            }
        }
    }

    Ok(())
}
