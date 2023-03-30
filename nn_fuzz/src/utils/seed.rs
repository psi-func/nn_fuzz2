use itertools::Itertools;
use serde::{Deserialize, Serialize};
use libafl::prelude::Error;

pub type Seed = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seeds {
    pub cmdline: String,

    pub vals: Option<Vec<Seed>>,
}

impl Seeds {
    pub fn from_cmdline(args: &str) -> Result<Self, Error> {
        if args == "auto" {
            return Ok(Self {
                cmdline: args.to_string(),
                vals: None,
            });
        }
        let mut seeds: Vec<Seed> = vec![];
        let seeds_args: Vec<&str> = args.split(',').collect();

        for csv in seeds_args {
            let seed_range: Vec<&str> = csv.split('-').collect();
            if seed_range.len() == 1 {
                seeds.push(seed_range[0].parse::<u64>()?);
            } else if seed_range.len() == 2 {
                for x in seed_range[0].parse::<u64>()?..=(seed_range[1].parse::<u64>()?) {
                    seeds.push(x);
                }
            }
        }

        if seeds.is_empty() {
            return Err(Error::illegal_argument(format!(
                "No seeds specified! parsed: {args}"
            )));
        }

        Ok(Self {
            cmdline: args.to_string(),
            vals: Some(seeds.into_iter().unique().collect()),   
        })
    }
}
