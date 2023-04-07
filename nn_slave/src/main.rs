use nn_slave::error::Error;

fn main() {
    let options = nn_slave::cli::parse_args();

    match nn_slave::fuzz::fuzz(&options) {
        Ok(_) | Err(Error::ShuttingDown) => {
            println!("Congrat! Good bye");
        }
        Err(e) => {
            println!("Some error during fuzzing");
            println!("{}", e);
        }
    }
}
