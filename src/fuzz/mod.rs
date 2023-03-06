use std::marker::PhantomData;

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
pub fn fuzz(options: FuzzerOptions) -> Result<(), Error> {
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
                        message: format!("Loaded tokens {} from {:?}", tokens.len(), options.tokens),
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
