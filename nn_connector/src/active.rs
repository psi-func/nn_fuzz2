use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;

use libafl::prelude::{
    compress::GzipCompressor, CorpusId, LLMP_FLAG_COMPRESSED, LLMP_FLAG_INITIALIZED,
};

use crate::error::Error;

use nn_messages::{
    active::{RLProtoMessage, TcpNewMessage, TcpRequest, TcpResponce, COMPRESSION_THRESHOLD},
    recv_tcp_msg, send_tcp_msg,
};

const NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

pub struct FuzzConnector {
    stream: TcpStream,
    compressor: GzipCompressor,
    current_id: Option<CorpusId>,
    #[allow(unused)]
    port: u16,
}

impl FuzzConnector {
    pub fn new(model_name: String, port: u16) -> Result<Self, Error> {
        let stream = connect_to_fuzzer(model_name, port)?;

        Ok(Self {
            stream,
            compressor: GzipCompressor::new(COMPRESSION_THRESHOLD),
            current_id: None,
            port,
        })
    }

    pub fn recv_input(&mut self) -> Result<HashMap<String, Vec<u8>>, Error> {
        if let RLProtoMessage::Predict { id, input, map } = self.recv_from_fuzzer()? {
            self.current_id = Some(id);
            Ok(HashMap::from([
                ("input".to_string(), input),
                ("map".to_string(), map),
            ]))
        } else {
            Err(Error::illegal_state("incorrent hello message".to_string()))
        }
    }

    pub fn recv_map(&mut self) -> Result<HashMap<String, Vec<u8>>, Error> {
        match self.recv_from_fuzzer()? {
            RLProtoMessage::MapAfterMutation { id, input, map } => {
                if self.current_id.unwrap() != id {
                    return Err(Error::illegal_state(format!(
                        "Unexpected id from fuzzer {id}"
                    )));
                }
                Ok(HashMap::from([
                    ("input".to_string(), input),
                    ("map".to_string(), map),
                ]))
            }
            RLProtoMessage::Reward { id, score: _ } => {
                if self.current_id.unwrap() != id {
                    return Err(Error::illegal_state(format!(
                        "Unexpected id from fuzzer on stopping {id}"
                    )));
                }
                Err(Error::stop_iteration())
            }
            _ => Err(Error::illegal_state("incorrent hello message".to_string())),
        }
    }

    pub fn send_heatmap(&mut self, heatmap: Vec<u32>) -> Result<(), Error> {
        let msg = RLProtoMessage::HeatMap {
            id: self.current_id.unwrap(),
            idxs: heatmap,
        };
        self.send_to_fuzzer(&msg)
    }

    fn recv_from_fuzzer(&mut self) -> Result<RLProtoMessage, Error> {
        fn unpack_message(
            compressor: &GzipCompressor,
            msg: &TcpNewMessage,
        ) -> Result<RLProtoMessage, Error> {
            let compressed;

            let rl_bytes = if msg.flags & LLMP_FLAG_COMPRESSED == LLMP_FLAG_COMPRESSED {
                compressed = compressor.decompress(&msg.payload)?;
                &compressed
            } else {
                &msg.payload
            };

            postcard::from_bytes(rl_bytes.as_slice())
                .map_err(|_e| Error::serialize("not RLProto message".to_string()))
        }

        recv_tcp_msg(&mut self.stream)
            .and_then(std::convert::TryInto::try_into)
            .map_err(Error::from)
            .and_then(|msg: TcpNewMessage| unpack_message(&self.compressor, &msg))
    }

    fn send_to_fuzzer(&mut self, msg: &RLProtoMessage) -> Result<(), Error> {
        fn pack_message(
            compressor: &GzipCompressor,
            msg: &RLProtoMessage,
        ) -> Result<TcpNewMessage, Error> {
            let serialized = postcard::to_allocvec(&msg)?;
            let flags = LLMP_FLAG_INITIALIZED;

            let tcp_message = match compressor.compress(&serialized)? {
                Some(comp_buf) => TcpNewMessage {
                    flags: flags | LLMP_FLAG_COMPRESSED,
                    payload: comp_buf,
                },
                None => TcpNewMessage {
                    flags,
                    payload: serialized,
                },
            };

            Ok(tcp_message)
        }

        pack_message(&self.compressor, msg)
            .and_then(|msg| send_tcp_msg(&mut self.stream, &msg).map_err(Error::from))
    }
}

#[allow(clippy::doc_markdown)]
///
/// @startuml
/// Fuzzer -> RLModel: Hello
/// RLModel -> Fuzzer: NnHello
/// Fuzzer -> RLModel: Accept   
/// @enduml
fn connect_to_fuzzer(model_name: String, port: u16) -> Result<TcpStream, Error> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;

    // 1 - receive hello from fuzzer
    recv_tcp_msg(&mut stream)
        .and_then(std::convert::TryInto::try_into)
        .map_err(Error::from)
        .and_then(|msg: TcpResponce| {
            if let TcpResponce::Hello { .. } = msg {
                Ok(())
            } else {
                Err(Error::illegal_state("incorrent hello message".to_string()))
            }
        })?;

    let hello_msg = TcpRequest::Hello { name: model_name };

    // 2 - send hello from us
    send_tcp_msg(&mut stream, &hello_msg)?;

    // 3 - wait for accepting
    recv_tcp_msg(&mut stream)
        .and_then(std::convert::TryInto::try_into)
        .map_err(Error::from)
        .and_then(|msg: TcpResponce| {
            if let TcpResponce::Accepted { .. } = msg {
                Ok(())
            } else {
                Err(Error::illegal_state(
                    "got incorrent message while wait for accepting".to_string(),
                ))
            }
        })?;

    // set read timeout
    stream.set_read_timeout(Some(NN_BLOCK_TIME))?;

    Ok(stream)
}
