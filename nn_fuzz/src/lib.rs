#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

pub const MAP_SIZE: usize = 65536;

pub mod fuzz;
pub mod cli;
pub mod connector;
pub mod error;

mod components;
mod llmp;
mod launcher;
mod utils;



