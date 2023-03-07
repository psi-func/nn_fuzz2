use std::marker::PhantomData;
use std::path::Path;

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
    detail::fuzz(options)
}

fn load_tokens<EM, S>(state: &mut S, options: &FuzzerOptions, mgr: &mut EM) -> Result<(), Error>
where
    EM: EventFirer<State = S>,
    S: HasMetadata + UsesInput,
{
    if state.metadata().get::<Tokens>().is_none() && !options.tokens.is_empty() {
        let mut tokens = Tokens::new();
        // load tokens
        tokens = match tokens.add_from_files(&options.tokens) {
            Ok(tokens) => {
                mgr.fire(
                    state,
                    Event::Log {
                        severity_level: LogSeverity::Debug,
                        message: format!(
                            "Loaded tokens {} from {:?}",
                            tokens.len(),
                            options.tokens
                        ),
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

fn mutate_args(args: &[String], config: &Path, core_id: usize) -> Result<Vec<String>, Error> {
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::fs;

    #[derive(Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Mutation {
        Increment,
    }

    fn increment(arg: &str, core_id: usize) -> Result<String, Error> {
        match arg.parse::<usize>() {
            Ok(value) => Ok(format!("{}", value + core_id)),
            Err(e) => Err(Error::illegal_argument(e.to_string())),
        }
    }

    impl Mutation {
        fn mutate(&self, arg: &str, core_id: usize) -> Result<String, Error> {
            match *self {
                Mutation::Increment => increment(arg, core_id),
            }
        }
    }

    let works: HashMap<String, Mutation> =
        fs::read_to_string(config).map(|text| serde_json::from_str(&text).unwrap_or_else(|_| {
            panic!("Invalid configure file! {:?}", config);
        }))?;
    
        let new_args : Vec<String> = args.iter().map(|el| {
            match works.get(el) {
                Some(value) => value.mutate(el.as_str(), core_id).unwrap(),
                None => el.into(),
            }
        }).collect();
        Ok(new_args)
}
