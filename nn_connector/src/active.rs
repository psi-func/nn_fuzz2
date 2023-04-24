use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;

use libafl::prelude::CorpusId;

use crate::error::Error;

use nn_messages::{
    active::{RLProtoMessage, TcpRequest, TcpResponce},
    recv_tcp_msg, send_tcp_msg,
};

const _LLMP_NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

pub struct FuzzConnector {
    stream: TcpStream,
    current_id: Option<CorpusId>,
    #[allow(unused)]
    port: u16,
}

impl FuzzConnector {
    pub fn new(model_name: String, port: u16) -> Result<Self, Error> {
        let stream = connect_to_fuzzer(model_name, port)?;

        Ok(Self {
            stream,
            current_id: None,
            port,
        })
    }

    pub fn recv_from_fuzzer(&mut self) -> Result<RLProtoMessage, Error> {
        recv_tcp_msg(&mut self.stream)
            .and_then(std::convert::TryInto::try_into)
            .map_err(Error::from)
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
            RLProtoMessage::Reward { id, score } => {
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
        send_tcp_msg(&mut self.stream, &msg).map_err(Error::from)
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
    stream.set_read_timeout(Some(_LLMP_NN_BLOCK_TIME))?;

    Ok(stream)
}
