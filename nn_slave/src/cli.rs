use std::path::PathBuf;
use std::time::Duration;

use clap::{self, Parser};

use libafl::{Error, prelude::CoreId};

#[must_use]
pub fn parse_args() -> SlaveOptions {
    SlaveOptions::parse()
}

fn parse_timeout(src: &str) -> Result<Duration, Error> {
    Ok(Duration::from_millis(src.parse()?))
}

#[allow(dead_code)]
fn parse_core(src: &str) -> Result<CoreId, Error> {
    Ok( CoreId(src.parse()? ))
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
pub struct SlaveOptions {
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

    /// Core to which client binds
    #[arg(
        short,
        long,
        default_value = "0",
    )]
    pub core: usize,

    /// The initial seed for RNG
    /// current nanos if unset
    #[arg(
        short,
        long,
        help_heading = "Fuzz Options",
    )]
    pub seed: Option<u64>,

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
    
    /// The file to write output from fuzzer instances
    #[arg(
        long, 
        value_name = "FILE",
        help_heading = "Fuzz Options")]
        pub stdout: Option<String>,
        
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
        default_value = "corpus_discovered/",
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

    /// The broker port to connect with  
    #[arg(
        long,
        default_value = "1337",
        value_name = "PORT",
        help_heading = "Connect Options",
    )]
    pub broker_port: u16,

    /// The NN connector port to connect with  
    #[arg(
        long,
        default_value = "7879",
        value_name = "PORT",
        help_heading = "Connect Options",
    )]
    pub port: u16,
}

