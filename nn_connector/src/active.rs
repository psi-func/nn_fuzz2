use std::net::TcpStream;
use std::time::Duration;

use crate::error::Error;

use nn_messages::{
    active::{RLProtoMessage, TcpRequest, TcpResponce},
    recv_tcp_msg, send_tcp_msg,
};

const _LLMP_NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

pub struct FuzzConnector {
    stream: TcpStream,
    #[allow(unused)]
    port: u16,
}

impl FuzzConnector {
    pub fn new(model_name: String, port: u16) -> Result<Self, Error> {
        let stream = connect_to_fuzzer(model_name, port)?;

        Ok(Self { stream, port })
    }

    pub fn recv_input(&mut self) -> Result<Vec<u8>, Error> {
        recv_tcp_msg(&mut self.stream)
            .and_then(std::convert::TryInto::try_into)
            .map_err(Error::from)
            .and_then(|msg: RLProtoMessage| {
                if let RLProtoMessage::Predict { input } = msg {
                    Ok(input)
                } else {
                    Err(Error::illegal_state(format!(
                        "Unexpected message type {msg:?}, while looking for Predict"
                    )))
                }
            })
    }

    pub fn send_heatmap(&mut self, heatmap: Vec<u32>) -> Result<(), Error> {
        let msg = RLProtoMessage::HeatMap { idxs: heatmap };
        send_tcp_msg(&mut self.stream, &msg).map_err(Error::from)
    }

    pub fn recv_reward(&mut self) -> Result<f64, Error> {
        recv_tcp_msg(&mut self.stream)
            .and_then(std::convert::TryInto::try_into)
            .map_err(Error::from)
            .and_then(|msg: RLProtoMessage| {
                if let RLProtoMessage::Reward { score } = msg {
                    Ok(score)
                } else {
                    Err(Error::illegal_state(format!(
                        "Unexpected message type {msg:?}, while looking for Predict"
                    )))
                }
            })
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
