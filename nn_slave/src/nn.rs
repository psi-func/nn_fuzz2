use libafl::prelude::{CorpusId, HasBytesVec, Input};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    runtime::Builder,
    sync::mpsc,
};

use nn_messages::active::{RLProtoMessage, TcpRequest, TcpResponce};
use nn_messages::error::Error as MsgError;

use crate::{cli::SlaveOptions, error::Error};

pub mod mutatios;

#[derive(Debug)]
pub enum Task<I>
where
    I: Input,
{
    Predict {
        id: CorpusId,
        input: I,
        map: Vec<u8>,
    },
    AfterMutCoverage {
        id: CorpusId,
        #[cfg(feature = "debug_mutations")]
        input: I,
        map: Vec<u8>,
    },
    MutEnd {
        id: CorpusId,
        score: f64,
    },
}

#[derive(Debug)]
pub enum TaskCompletion {
    Prediction { id: CorpusId, heatmap: Vec<u32> },
    NnDropped,
}

#[derive(Debug)]
pub struct NeuralNetwork<I>
where
    I: Input,
{
    send: mpsc::Sender<Task<I>>,
    recv: mpsc::Receiver<TaskCompletion>,
}

impl<I> NeuralNetwork<I>
where
    I: Input + HasBytesVec + std::marker::Send + 'static,
{
    pub fn new(options: &SlaveOptions) -> Self {
        let (sender, receiver) = mpsc::channel(300);
        let (send_back, receive_back) = mpsc::channel(300);

        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        let port = options.port;
        std::thread::spawn(move || {
            rt.block_on(async move {
                let mut service: NnService<I> = NnService::on_port(port, send_back, receiver);
                match service.run_service().await {
                    Ok(_) => (),
                    Err(e) => panic!("Error in nn service: {e}"),
                }
            });
        });

        NeuralNetwork {
            send: sender,
            recv: receive_back,
        }
    }

    pub fn predict(&self, id: CorpusId, input: I, map: Vec<u8>) -> Result<(), Error> {
        self.send
            .blocking_send(Task::Predict { id, input, map })
            .map_err(|e| Error::illegal_state(format!("Couldn't send input! {e}")))?;
        Ok(())
    }

    #[cfg(feature = "debug_mutations")]
    pub fn rl_step(&self, id: CorpusId, input: I, map: Vec<u8>) -> Result<(), Error> {
        self.send
            .blocking_send(Task::AfterMutCoverage { id, input, map })
            .map_err(|e| Error::illegal_state(format!("Couldn't send step! {e}")))?;
        Ok(())
    }

    #[cfg(not(feature = "debug_mutations"))]
    pub fn rl_step(&self, id: CorpusId, map: Vec<u8>) -> Result<(), Error> {
        self.send
            .blocking_send(Task::AfterMutCoverage { id, map })
            .map_err(|e| Error::illegal_state(format!("Couldn't send step! {e}")))?;
        Ok(())
    }

    pub fn calc_reward(&self, id: CorpusId) -> Result<(), Error> {
        self.send
            .blocking_send(Task::MutEnd { id, score: 0_f64})
            .map_err(|e| Error::illegal_state(format!("Couldn't send step! {e}")))?;
        Ok(())
    }

    pub fn nn_responce(&mut self) -> Option<TaskCompletion> {
        match self.recv.try_recv() {
            Ok(prediction) => Some(prediction),
            Err(e) => {
                match e {
                    // maybe respawn neural network
                    mpsc::error::TryRecvError::Disconnected | mpsc::error::TryRecvError::Empty => {
                        None
                    }
                }
            }
        }
    }
}

const _BIND_ADDR: &str = "127.0.0.1";

enum State {
    Listening,
    Active,
}

struct NnService<I>
where
    I: Input,
{
    nn_name: Option<String>,
    port: u16,
    send: mpsc::Sender<TaskCompletion>,
    recv: mpsc::Receiver<Task<I>>,
    state: State,
}

impl<I> NnService<I>
where
    I: Input + std::marker::Send + 'static,
{
    #[allow(dead_code)]
    fn new(send: mpsc::Sender<TaskCompletion>, recv: mpsc::Receiver<Task<I>>) -> Self {
        Self {
            nn_name: None,
            send,
            recv,
            port: 0,
            state: State::Listening,
        }
    }

    fn on_port(
        port: u16,
        send: mpsc::Sender<TaskCompletion>,
        recv: mpsc::Receiver<Task<I>>,
    ) -> Self {
        Self {
            nn_name: None,
            send,
            recv,
            port,
            state: State::Listening,
        }
    }
}

impl<I> NnService<I>
where
    I: Input + HasBytesVec + std::marker::Send + 'static,
{
    pub async fn run_service(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind((_BIND_ADDR, self.port)).await?;
        self.state = State::Listening;

        let mut stream: Option<TcpStream> = None;

        loop {
            if let State::Listening = self.state {
                let (mut ss, _) = listener.accept().await?;

                // 1 - meet nn
                let hello = TcpResponce::Hello {
                    name: format!("nn_slave_{}", self.port),
                };
                send_tcp_message(&mut ss, &hello).await?;

                // 2 - get nn info
                #[allow(irrefutable_let_patterns)]
                let nn_name = recv_tcp_message(&mut ss)
                    .await
                    .map_err(MsgError::from)
                    .and_then(std::convert::TryInto::try_into)
                    .map_err(|e| Error::serialize(format!("NNService: incorrect message: {e}")))
                    .and_then(|msg: TcpRequest| {
                        if let TcpRequest::Hello { name } = msg {
                            Ok(name)
                        } else {
                            Err(Error::illegal_state(
                                "NNService: Incorrrect message type while handshaking!".to_string(),
                            ))
                        }
                    })?;
                if let Some(nn_id) = self.nn_name.as_ref() {
                    if *nn_id != nn_name {
                        let responce = TcpResponce::Error {
                            description: format!("Accepted {nn_name}, while {nn_id} expected..."),
                        };
                        send_tcp_message(&mut ss, &responce).await?;
                        continue;
                    }
                } else {
                    self.nn_name = Some(nn_name);
                }

                // 3 - send acceptance msg
                send_tcp_message(&mut ss, &TcpResponce::Accepted {}).await?;

                // Ok, go further
                stream = Some(ss);
                println!(
                    "[NN] {} connected to fuzzer",
                    self.nn_name.as_ref().unwrap()
                );
                self.state = State::Active;
            }

            if let State::Active = self.state {
                match self.handle_connection(stream.take().unwrap()).await {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Some error with client {e} restarting...");
                        self.send.send(TaskCompletion::NnDropped).await.unwrap();
                        self.state = State::Listening;
                    }
                }
            }
        }
    }

    async fn handle_connection(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        let stream = &mut stream;

        loop {
            match self.recv.recv().await {
                Some(Task::Predict { id, input, map }) => {
                    let msg = RLProtoMessage::Predict {
                        id,
                        input: input.bytes().to_vec(),
                        map,
                    };

                    send_tcp_message(stream, &msg).await?;

                    // wait for network responce
                    let (id, heatmap) = recv_tcp_message(stream)
                        .await
                        .map_err(MsgError::from)
                        .and_then(std::convert::TryInto::try_into)
                        .map_err(|e| Error::serialize(format!("NNService: incorrect message: {e}")))
                        .and_then(|msg: RLProtoMessage| {
                            if let RLProtoMessage::HeatMap { id, idxs } = msg {
                                Ok((id, idxs))
                            } else {
                                Err(Error::illegal_state(
                                    "NNService: Incorrrect message type while handshaking!"
                                        .to_string(),
                                ))
                            }
                        })?;

                    // push to fuzzer
                    self.send
                        .send(TaskCompletion::Prediction { id, heatmap })
                        .await
                        .map_err(|e| Error::illegal_state(format!("Couldn't send reward! {e}")))?;
                }
                #[cfg(feature = "debug_mutations")]
                Some(Task::AfterMutCoverage { id, input, map }) => {
                    let msg = RLProtoMessage::MapAfterMutation { id, input, map };

                    send_tcp_message(stream, &msg).await?;
                }
                #[cfg(not(feature = "debug_mutations"))]
                Some(Task::AfterMutCoverage { id, map }) => {
                    let msg = RLProtoMessage::MapAfterMutation {
                        id,
                        input: vec![],
                        map,
                    };
                    send_tcp_message(stream, &msg).await?;
                }
                Some(Task::MutEnd {id, score }) => {
                    let msg = RLProtoMessage::Reward { id, score };
                    send_tcp_message(stream, &msg).await?;
                }
                None => return Ok(()),
            }
        }
    }
}

/*
* Helper functions
*/
async fn recv_tcp_message(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut size_bytes = [0u8; 4];
    stream.read_exact(&mut size_bytes).await?;
    let size = u32::from_be_bytes(size_bytes);
    let mut bytes = Vec::new();
    bytes.resize(size as usize, 0);

    stream
        .read_exact(&mut bytes)
        .await
        .expect("Failed to read message body");

    Ok(bytes)
}

async fn send_tcp_message<T>(stream: &mut TcpStream, msg: &T) -> std::io::Result<()>
where
    T: Serialize,
{
    let msg = postcard::to_allocvec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
    if let Ok(len) = u32::try_from(msg.len()) {
        let size_bytes = len.to_be_bytes();
        stream.write_all(&size_bytes).await?;
        stream.write_all(&msg).await?;
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Trying to send a tcp message > u32 (size: {})", msg.len()),
        ))
    }
}
