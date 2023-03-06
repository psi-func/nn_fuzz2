use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use nn_fuzz;
use nn_fuzz::error::Error;

fn main() {
    let opts = nn_fuzz::cli::parse_args();    
    match nn_fuzz::fuzz::fuzz(opts) {
        Ok(_) | Err(Error::ShuttingDown) => {
            println!("Congrat! Good bye");
        },
        Err(_) => {
            println!("Some error during fuzzing");
        }
    }
}
