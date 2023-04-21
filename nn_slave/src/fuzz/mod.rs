use std::marker::PhantomData;
use std::path::PathBuf;

use libafl::prelude::*;

use crate::cli::SlaveOptions;

#[cfg_attr(target_family = "windows", path = "win.rs")]
#[cfg_attr(target_family = "unix", path = "unix.rs")]
mod detail;

///
/// # Errors
///
/// - ``Error::ShuttingDown`` when fuzzer stops normally
///
pub fn fuzz(options: &SlaveOptions) -> Result<(), Error> {
    detail::fuzz(options)
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