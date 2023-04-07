#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![deny(clippy::cargo_common_metadata)]
#![deny(clippy::all, clippy::pedantic)]

use error::Error;
use libafl::prelude::Flags;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::TcpStream;

pub const LLMP_FLAG_INITIALIZED: Flags = 0x0;
pub const LLMP_FLAG_FROM_NN: Flags = 0x4;
pub const LLMP_FLAG_COMPRESSED: Flags = 0x1;

/// The minimum buffer size at which to compress LLMP IPC messages.
pub const COMPRESS_THRESHOLD: usize = 1024;

pub mod active;
pub mod error;
pub mod passive;

/// Send one message of `u32` len and `[u8; len]` bytes
///
/// # Errors
///    
///  ``illegal_state`` if length is more than u32
pub fn send_tcp_msg<T>(stream: &mut TcpStream, msg: &T) -> Result<(), Error>
where
    T: Serialize,
{
    let msg = postcard::to_allocvec(msg)?;
    if let Ok(len) = u32::try_from(msg.len()) {
        let size_bytes = len.to_be_bytes();
        stream.write_all(&size_bytes)?;
        stream.write_all(&msg)?;
        Ok(())
    } else {
        Err(Error::io_error("Too large packet".into()))
    }
}
/// Receive one message of `u32` len and `[u8; len]` bytes
///
/// # Errors
///
/// ``illegal_state`` if length is more than u32
pub fn recv_tcp_msg(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    // Always receive one be u32 of size, then the command.
    let mut size_bytes = [0_u8; 4];
    if let Err(e) = stream.read_exact(&mut size_bytes) {
        return Err(match e.kind() {
            std::io::ErrorKind::TimedOut => Error::not_available(),
            _ => Error::io_error(e.to_string()),
        });
    }
    let size = u32::from_be_bytes(size_bytes);
    let mut bytes = vec![];
    bytes.resize(size as usize, 0_u8);

    match stream.read_exact(&mut bytes) {
        Ok(_) => Ok(bytes),
        Err(e) => Err(Error::io_error(e.to_string())),
    }
}
