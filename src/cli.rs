use std::path::PathBuf;
use std::time::Duration;

use clap::{self, Parser};

#[allow(unused_imports)]
use serde::{Deserialize, Serialize};

use libafl::{Error, prelude::Cores};

#[must_use]
pub fn parse_args() -> FuzzerOptions {
    FuzzerOptions::parse()
}

fn parse_timeout(src: &str) -> Result<Duration, Error> {
    Ok(Duration::from_millis(src.parse()?))
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
pub struct FuzzerOptions {
    #[arg(
        help = "The instrumented binary we want to fuzz",
        name = "EXEC",
        required = true,
    )]
    pub executable: PathBuf,

    #[arg(
        help = "The arguments passed to target",
        num_args(1..),
        allow_hyphen_values = true,
    )]
    pub args: Vec<String>,

    /// Spawn a client in each of the provided cores. Use 'all' to select all available
    /// cores. 'none' to run a client without binding to any core.
    /// ex: '1,2-4,6' selects the cores 1, 2, 3, 4, and 6.
    #[arg(
        short,
        long,
        default_value = "0",
        value_parser = Cores::from_cmdline,
        help = "The list of cores where spawn fuzzers"
    )]
    pub cores: Cores,

    #[arg(
        long,
        value_name = "FILE",
        help = "The file which describes how to mutate args in inferior using `core_id`",
    )]
    pub core_args_config : Option<PathBuf>,
    
    #[arg(
        long, 
        value_name = "FILE",
        help = "The file to write output from fuzzer instances",
        help_heading = "Fuzz Options")]
    pub stdout: Option<String>,
    
    #[arg(
        short,
        long,
        help = "The initial seed value for random generator, current_nanos if undefined",
        help_heading = "Fuzz Options",
    )]
    pub seed: Option<u64>,

    #[arg(
        short, 
        long,
        value_parser = parse_timeout,
        default_value = "1000", 
        help = "The timeout for each input execution (millis)",
        help_heading = "Fuzz Options",
    )]
    pub timeout: Duration,

    #[arg(
        short = 'x',
        long,
        value_name = "FILE",
        help = "The token file for token mutations",
        help_heading = "Fuzz Options",
    )]
    pub tokens: Vec<PathBuf>,

    #[arg(
        short,
        long,
        help = "If not set, the child's stdout and stderror will be redirected to /dev/null",
        help_heading = "Fuzz Options"
    )]
    pub debug_child: bool,

    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "The directory to read initial corpus, generate inputs if undefined",
        help_heading = "Corpus Options",
    )]
    pub input: Option<Vec<PathBuf>>,

    #[arg(
        short,
        long,
        value_name = "PATH",
        default_value = "solutions/",
        help = "The directory where solutions are stored",
        help_heading = "Corpus Options",
    )]
    pub output: PathBuf,

    #[arg(
        long,
        default_value = "20",
        help = "The number of generated inputs (used only if no input)",
        help_heading = "Corpus Options",
    )]
    pub generate_count: usize,

    #[arg(
        long,
        default_value = "4096",
        help = "The maximum length of generated inputs (used only if no input)",
        help_heading = "Corpus Options",
    )]
    pub input_max_length: usize,
    
    #[arg(
        long,
        default_value = "1337",
        value_name = "PORT",
        help = "The port broker listen to accept new instances",
        help_heading = "Broker Options",
    )]
    pub broker_port: u16,
    
    #[arg(
        short = 'S',
        long,
        help = "If set, spawn message passing with remote receivers",
        help_heading = "Broker Options",
    )]
    pub spawn_client: bool,

    #[arg(
        short = 'p',
        long,
        default_value = "7878",
        value_name = "PORT",
        help = "",
        help_heading = "Broker Options",
    )]
    pub client_port: u16,


}
