use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::error::Error;

use nn_lib::connector::messages::{
    TcpRemoteNewMessage, TcpRequest, TcpResponce,
    COMPRESS_THRESHOLD,
    LLMP_FLAG_COMPRESSED, LLMP_FLAG_INITIALIZED,
};

use libafl::prelude::{ExitKind, EventConfig};
use postcard;
use serde::Serialize;

#[allow(unused)]
use libafl::prelude::{
    BytesInput, ClientId, Event, Flags, GzipCompressor, HasBytesVec, Input, Tag,
};

const _LLMP_NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

pub struct FuzzConnector {
    compressor: GzipCompressor,
    client_id: ClientId,
    stream: TcpStream,
    #[allow(unused)]
    port: u16,
}

impl FuzzConnector {
    pub fn new(port: u16) -> Result<Self, Error> {
        let (stream, client_id) = connect_to_fuzzer(port)?;

        Ok(Self {
            port,
            stream,
            client_id,
            compressor: GzipCompressor::new(COMPRESS_THRESHOLD),
        })
    }

    pub fn send_input(&mut self, input: &[u8]) -> Result<(), Error> {
        let testcase = generate_event(self.client_id, &self.compressor, input)?;
        send_tcp_msg(&mut self.stream, &testcase)
    }

    pub fn recv_testcase(&mut self) -> Result<HashMap<String, Vec<u8>>, Error> {
        recv_event::<BytesInput>(&mut self.stream, &self.compressor).map(|event| match event {
            Event::NewTestcase {
                input,
                observers_buf: Some(observers_buf),
                ..
            } => HashMap::from([
                ("input".to_string(), input.bytes().to_owned()),
                ("observers".to_string(), observers_buf),
            ]),
            _ => HashMap::from([("input".to_string(), vec![0_u8])]),
        })
    }
}

pub fn connect_to_fuzzer(port: u16) -> Result<(TcpStream, ClientId), Error> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;

    // 1 - receive hello from fuzzer
    recv_tcp_msg(&mut stream)
        .and_then(|buf| buf.try_into().or(Err(Error::serialize_error("Hello message serialize error".into()))))
        .and_then(|msg: TcpResponce| {
            if let TcpResponce::RemoteFuzzerHello { .. } = msg {
                Ok(())
            } else {
                Err(Error::illegal_state("incorrent hello message".to_string()))
            }
        })?;

    let hello_msg = TcpRequest::RemoteNnHello {
        nn_name: "markov_chain".to_string(),
        nn_version: "1.0".to_string(),
    };

    // 2 - send hello from us
    send_tcp_msg(&mut stream, &hello_msg)?;

    // 3 - wait for accepting
    let client_id = recv_tcp_msg(&mut stream)
        .and_then(|buf| buf.try_into().or(Err(Error::serialize_error("Accept message serialize error".into()))))
        .and_then(|msg: TcpResponce| {
            if let TcpResponce::RemoteNNAccepted { client_id } = msg {
                Ok(client_id)
            } else {
                Err(Error::illegal_state(
                    "got incorrent message while wait for accepting".to_string(),
                ))
            }
        })?;

    // set read timeout
    stream.set_read_timeout(Some(_LLMP_NN_BLOCK_TIME))?;

    // return prepared stream
    Ok((stream, client_id))
}

pub fn generate_event(
    client_id: ClientId,
    compressor: &GzipCompressor,
    buf: &[u8],
) -> Result<TcpRemoteNewMessage, Error> {
    let event = Event::<BytesInput>::NewTestcase { 
        input: BytesInput::from(buf), 
        observers_buf: None, 
        exit_kind: ExitKind::Ok, 
        corpus_size: 0, 
        client_config: EventConfig::AlwaysUnique, 
        time: Duration::from_millis(1), 
        executions: 0, 
    };

    let serialized = postcard::to_allocvec(&event)?;
    let flags: Flags = LLMP_FLAG_INITIALIZED;

    let testcase = match compressor.compress(&serialized)? {
        Some(comp_buf) => TcpRemoteNewMessage {
            client_id,
            tag: Default::default(),
            flags: flags | LLMP_FLAG_COMPRESSED,
            payload: comp_buf,
        },
        None => TcpRemoteNewMessage {
            client_id,
            tag: Default::default(),
            flags,
            payload: serialized,
        },
    };

    Ok(testcase)
}

/// Assumed that stream has timeout enabled
pub fn recv_event<I: Input>(
    stream: &mut TcpStream,
    compressor: &GzipCompressor,
) -> Result<Event<I>, Error> {
    let msg: TcpRemoteNewMessage = match recv_tcp_msg(stream) {
        Ok(buf) => buf.try_into()?,
        Err(_e) => {
            return Err(Error::not_available());
        }
    };
    let compressed;

    let event_bytes = if msg.flags & LLMP_FLAG_COMPRESSED == LLMP_FLAG_COMPRESSED {
        compressed = compressor.decompress(&msg.payload)?;
        &compressed
    } else {
        &msg.payload
    };

    postcard::from_bytes(event_bytes.as_slice()).map_err(|_e| {
        Error::serialize_error("not Event<BytesInput> message".to_string())
    })
}

// helper functions
fn send_tcp_msg<T>(stream: &mut TcpStream, msg: &T) -> Result<(), Error>
where
    T: Serialize,
{
    let msg = postcard::to_allocvec(msg)?;
    if msg.len() > u32::MAX as usize {
        return Err(Error::illegal_state(format!(
            "Trying to send message a tcp message > u32! (size: {})",
            msg.len()
        )));
    }

    let size_bytes = (msg.len() as u32).to_be_bytes();
    stream.write_all(&size_bytes)?;
    stream.write_all(&msg)?;

    Ok(())
}

/// Receive one message of `u32` len and `[u8; len]` bytes
fn recv_tcp_msg(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    // Always receive one be u32 of size, then the command.

    let mut size_bytes = [0_u8; 4];
    stream.read_exact(&mut size_bytes)?;
    let size = u32::from_be_bytes(size_bytes);
    let mut bytes = vec![];
    bytes.resize(size as usize, 0_u8);

    stream
        .read_exact(&mut bytes)
        .expect("Failed to read message body");
    Ok(bytes)
}
