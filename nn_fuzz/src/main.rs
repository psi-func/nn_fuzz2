use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use nn_fuzz::error::Error;

#[cfg(not(feature="net_monitor"))]
use env_logger::Env;

fn main() { 
    #[cfg(not(feature="net_monitor"))] 
    let env = Env::new().filter("FUZZ_LOG").write_style("FUZZ_LOG_STYLE");
    #[cfg(not(feature="net_monitor"))]
    env_logger::init_from_env(env);
    let opts = nn_fuzz::cli::parse_args();
    match nn_fuzz::fuzz::fuzz(&opts) {
        Ok(_) | Err(Error::ShuttingDown) => {
            println!("Congrat! Good bye");
        }
        Err(e) => {
            println!("Some error during fuzzing");
            println!("{}", e);
        }
    }
}
