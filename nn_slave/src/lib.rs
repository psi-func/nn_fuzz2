#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

pub const MAP_SIZE: usize = 65536;

mod components;
mod nn;
mod launcher;

pub mod cli;
pub mod fuzz;
pub mod error;

