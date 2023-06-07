use std::{path::PathBuf, ffi::OsString};
use std::time::Duration;

use clap::{self, Parser};

use libafl::{Error, prelude::Cores};

use crate::utils::seed::Seeds;

#[must_use]
pub fn parse_args() -> FuzzerOptions {
    FuzzerOptions::parse()
}

fn parse_timeout(src: &str) -> Result<Duration, Error> {
    Ok(Duration::from_millis(src.parse()?))
}

fn parse_env(src: &str) -> Result<(OsString, OsString), Error> {
    match src.find("=") {
        Some(place) => {
            let (key, value) = src.split_at(place);
            Ok((key.into(), value[1..].into()))
        },
        None => {
            Err(Error::serialize(format!("Incorrect env setting {}", src)))
        }
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
pub struct FuzzerOptions {

    /// The instrumented binary we want to fuzz
    #[arg(
        name = "EXEC",
        required = true,
    )]
    pub executable: PathBuf,

    /// The arguments passed to target
    #[arg(
        num_args(1..),
        allow_hyphen_values = true,
    )]
    pub args: Vec<String>,

    /// Run harness with user-provided environment variables
    /// ex: ASAN_OPTIONS=abort_on_error=1:error_code=0:detect_leaks=1
    #[arg(
        short,
        long = "env",
        value_parser = parse_env
    )]
    pub envs: Vec<(OsString, OsString)>,

    /// Spawn a client in each of the provided cores. Use 'all' to select all available
    /// cores. 'none' to run a client without binding to any core.
    /// ex: '1,2-4,6' selects the cores 1, 2, 3, 4, and 6.
    #[arg(
        short,
        long,
        default_value = "0",
        value_parser = Cores::from_cmdline,
    )]
    pub cores: Cores,

    /// The file which describes how to mutate args in inferior using `core_id`
    #[arg(
        long,
        value_name = "FILE",
    )]
    pub core_args_config : Option<PathBuf>,
    
    /// The file to write output from fuzzer instances
    #[arg(
        long, 
        value_name = "FILE",
        help_heading = "Fuzz Options")]
    pub stdout: Option<String>,
    
    /// The list of seeds for random generator per core, current_nanos if "auto"
    /// Must be not less than cores list len!
    /// 
    /// Example: 703,12,0-10
    #[arg(
        short,
        long,
        default_value = "auto",
        value_parser = Seeds::from_cmdline,
        help_heading = "Fuzz Options",
    )]
    pub seed: Seeds,

    /// The flag which enables usage backtrace information to make crashes unique
    #[arg(short, long, help_heading = "Fuzz Options",)]
    pub backtrace : bool,

    /// The timeout for each input execution (millis)
    #[arg(
        short, 
        long,
        value_parser = parse_timeout,
        default_value = "1000", 
        help_heading = "Fuzz Options",
    )]
    pub timeout: Duration,

    /// The token file for token mutations
    #[arg(
        short = 'x',
        long,
        value_name = "FILE",
        help_heading = "Fuzz Options",
    )]
    pub tokens: Vec<PathBuf>,

    /// If not set, the child's stdout and stderror will be redirected to /dev/null
    #[arg(
        short,
        long,
        help_heading = "Fuzz Options"
    )]
    pub debug_child: bool,

    /// The directory to read initial corpus, generate inputs if undefined
    #[arg(
        short,
        long,
        value_name = "PATH",
        help_heading = "Corpus Options",
    )]
    pub input: Option<Vec<PathBuf>>,

    /// The directory where solutions are stored
    #[arg(
        short,
        long,
        value_name = "PATH",
        default_value = "solutions/",
        help_heading = "Corpus Options",
    )]
    pub output: PathBuf,

    /// The directory where corpus is stored
    #[arg(
        short,
        long,
        value_name = "PATH",
        default_value = "corpus_discovered",
        help_heading = "Corpus Options",
    )]
    pub queue: PathBuf,

    /// The number of generated inputs (used only if no input)
    #[arg(
        long,
        default_value = "20",
        help_heading = "Corpus Options",
    )]
    pub generate_count: usize,

    /// The maximum length of generated inputs (used only if no input)
    #[arg(
        long,
        default_value = "4096",
        help_heading = "Corpus Options",
    )]
    pub input_max_length: usize,
    
    /// If not set, spawn broker for fuzzers
    #[arg(
        short = 'B',
        long,
        help_heading = "Broker Options",
    )]
    pub no_broker : bool,

    /// The port broker listen to accept new instances
    #[arg(
        long,
        default_value = "1337",
        value_name = "PORT",
        help_heading = "Broker Options",
    )]
    pub broker_port: u16,
    
    /// If set, spawn message passing with remote receivers
    #[arg(
        short = 'S',
        long,
        help_heading = "Broker Options",
    )]
    pub spawn_client: bool,

    /// The port to which nn-client will be bind
    #[arg(
        short = 'p',
        long,
        default_value = "7878",
        value_name = "PORT",
        help_heading = "Broker Options",
    )]
    pub client_port: u16,


}
