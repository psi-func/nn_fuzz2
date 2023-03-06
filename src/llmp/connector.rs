use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot::Sender;

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::net::TcpStream as StdTcpStream;
use std::time::Duration;

use libafl::bolts::llmp::{ClientId, Flags, LlmpDescription, LlmpReceiver, LlmpSender, Tag};
use libafl::bolts::shmem::{ShMem, ShMemDescription, ShMemProvider};
use libafl::Error;

use serde::{Deserialize, Serialize};

pub const LLMP_FLAG_FROM_NN: Flags = 0x4;

const _MAX_WORKING_THREADS: usize = 2;
const _LLMP_NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

#[cfg(feature = "bind_public")]
const _BIND_ADDR: &str = "0.0.0.0";

#[cfg(not(feature = "bind_public"))]
const _BIND_ADDR: &str = "127.0.0.1";

/// Messages for nn connection.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TcpRemoteNewMessage {
    /// the client ID of the fuzzer
    client_id: ClientId,
    /// the message tag
    tag: Tag,
    /// the flags
    flags: Flags,
    /// actual content of message
    payload: Vec<u8>,
}

impl TryFrom<&Vec<u8>> for TcpRemoteNewMessage {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(bytes)?)
    }
}

impl TryFrom<Vec<u8>> for TcpRemoteNewMessage {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(&bytes)?)
    }
}

/// Handshake over NN and Fuzzer
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TcpResponce {
    /// After receiving new connection, the broker send hello
    RemoteFuzzerHello { fuzz_description: FuzzerDescription },
    // Notify the client nn that it has been accepted
    RemoteNNAccepted {
        /// The clientId this client should send messages as.
        client_id: ClientId,
    },
    /// Something went wrong when processing the request
    Error {
        /// Error description
        description: String,
    },
}

impl TryFrom<&Vec<u8>> for TcpResponce {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(bytes.as_slice())?)
    }
}

impl TryFrom<Vec<u8>> for TcpResponce {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(bytes.as_slice())?)
    }
}

/// Response for requests to the nn
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TcpRequest {
    /// After sending hello wait for hello from nn
    RemoteNnHello {
        /// Additional info about nn env and settings
        nn_name: String,
        nn_version: String,
    },
    /// After sending hello wait for hello from local fuzzer instances
    LocalHello {
        /// Additional info about local fuzzer
        client_id: ClientId,
    },
}

impl TryFrom<Vec<u8>> for TcpRequest {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(bytes.as_slice())?)
    }
}

impl TryFrom<&Vec<u8>> for TcpRequest {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        Ok(postcard::from_bytes(bytes.as_slice())?)
    }
}

/// Info required by neural network to work with
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FuzzerDescription {
    /// edge coverage map size
    pub ec_size: usize,
    /// Running instances count
    pub instances: usize,
    /// Fuzzing target
    pub fuzz_target: String,
}

/// Info which NN provides before start
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NNDescription {
    pub nn_name: String,
    pub nn_version: String,
}

pub struct InitContext<SP>
where
    SP: ShMemProvider + 'static,
{
    pub port: u16,
    pub broadcast_receiver: LlmpReceiver<SP>,
    pub sender: LlmpSender<SP>,
}

#[derive(Debug)]
enum Listener {
    Tcp(TcpListener),
}

impl Listener {
    async fn accept(&self) -> ListenerStream {
        match self {
            Listener::Tcp(inner) => match inner.accept().await {
                Ok(res) => ListenerStream::Tcp(res.0, res.1),
                Err(err) => ListenerStream::Empty,
            },
        }
    }
}

#[derive(Debug)]
pub enum ListenerStream {
    Tcp(TcpStream, SocketAddr),
    Empty,
}

pub async fn run_service<SP: ShMemProvider + 'static>(
    sender: Sender<ShMemDescription>,
    broker_shmem_description: ShMemDescription,
    client_id: ClientId,
    port: u16,
) {
    let listener = Listener::Tcp(
        TcpListener::bind((_BIND_ADDR, port))
            .await
            .expect(&format!("NN connector: Cannot bind to port: {port}")),
    );

    let hello = TcpResponce::RemoteFuzzerHello {
        fuzz_description: FuzzerDescription {
            ec_size: crate::MAP_SIZE,
            // TODO: get real values
            instances: 0,
            fuzz_target: "".to_string(),
        },
    };

    let mut sender = Some(sender);

    loop {
        match listener.accept().await {
            ListenerStream::Tcp(mut stream, addr) => {
                match send_tcp_message(&mut stream, &hello).await {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Error sending initial hello: {e:?}");
                        continue;
                    }
                }

                let buf = match recv_tcp_message(&mut stream).await {
                    Ok(buf) => buf,
                    Err(e) => {
                        eprintln!("Error receiving from tcp: {e:?}");
                        continue;
                    }
                };

                let req = match buf.try_into() {
                    Ok(req) => req,
                    Err(e) => {
                        eprintln!("Could not deserialize tcp message: {:?}", e);
                        continue;
                    }
                };

                // handle connection
                match req {
                    // remote nn connection
                    TcpRequest::RemoteNnHello { nn_name, nn_version } => {
                        if let Some(send) = sender.take() {
                            let client_id = u32::MAX;

                            let msg = TcpResponce::RemoteNNAccepted { client_id };

                            match send_tcp_message(&mut stream, &msg).await {
                                Err(e) => println!("Cannot send message"),
                                _ => ()
                            }

                            tokio::task::spawn_blocking(move || {
                                let mut nn_connector: NnConnector<SP> =
                                    NnConnector::new(broker_shmem_description, client_id, send);

                                nn_connector.handle_connection(stream, NNDescription { nn_name, nn_version });
                            });

                            // tokio::spawn(async move {
                                //     let nn_connector: NnConnector<SP> =
                                //         NnConnector::new(broker_shmem_description, client_id, send);
                                
                                //     nn_connector.handle_connection(
                            //         stream,
                            //         description,

                            //     )
                            //     .await;
                            // });
                        } else {
                            eprintln!("Can only connect with one nn");
                        }
                    }
                    // local fuzzer connection
                    TcpRequest::LocalHello { client_id } => {
                        tokio::spawn(async move {
                            let mut fuzz_connector = InstanceConnector::new(client_id);
                            
                            fuzz_connector.handle_connection(stream).await;
                        });
                    }
                }
            }
                
            // Just ignore faults
            ListenerStream::Empty => {
                continue;
            }
        } // end loop
    }
}

struct InstanceConnector {
    id: ClientId,
}

impl InstanceConnector {
    fn new(client_id: ClientId) -> Self {
        InstanceConnector { id: client_id }
    }

    async fn handle_connection(&mut self, stream: TcpStream) {
        // setup

        loop {} // end loop
    }
}

struct NnConnector<SP: ShMemProvider + 'static> {
    id: ClientId,
    receiver: LlmpReceiver<SP>,
    sender: LlmpSender<SP>,
}

impl<SP> NnConnector<SP>
where
    SP: ShMemProvider + 'static,
{
    fn new(
        broker_shmem_desc: ShMemDescription,
        client_id: ClientId,
        send: Sender<ShMemDescription>,
    ) -> Self {
        let shmem_provider_bg = SP::new().unwrap();

        let mut new_sender = match LlmpSender::new(shmem_provider_bg.clone(), client_id, false) {
            Ok(new_sender) => new_sender,
            Err(e) => {
                panic!("NN connector: Could not map shared map: {e}");
            }
        };

        send.send(new_sender.out_shmems.first().unwrap().shmem.description())
            .expect("NN connector: Error sending map description to channel!");

        let local_receiver = LlmpReceiver::on_existing_from_description(
            shmem_provider_bg,
            &LlmpDescription {
                last_message_offset: None,
                shmem: broker_shmem_desc,
            },
        )
        .expect("NN connector: Failed to map local page in nn connector thread");

        NnConnector {
            id: client_id,
            receiver: local_receiver,
            sender: new_sender,
        }
    }

    fn handle_connection(&mut self, stream: TcpStream, desc: NNDescription) {
        // prepare stream
        let mut stream = transform_stream(stream).expect("Cannot transform stream");

        stream
            .set_read_timeout(Some(_LLMP_NN_BLOCK_TIME))
            .expect("Failed to set tcp stream timeout");

        loop {
            // first, forward all data we have.
            while let Some((client_id, tag, flags, payload)) = self
                .receiver
                .recv_buf_with_flags()
                .expect("Error reading from local page!")
            {
                if client_id == self.id {
                    // println!(
                    //     "Ignored message we probably sent earlier (same id), TAG: {:x}",
                    //     tag
                    // );
                    continue;
                }

                // We got a new message! Forward...
                send_tcp_msg(
                    &mut stream,
                    &TcpRemoteNewMessage {
                        client_id,
                        tag,
                        flags,
                        payload: payload.to_vec(),
                    },
                )
                .expect("Error sending message to nn");
            }

            // Then, see if we can receive something.
            // We set a timeout on the receive earlier.
            // This makes sure we will still forward our own stuff.
            // Forwarding happens between each recv, too, as simplification.
            // We ignore errors completely as they may be timeout, or stream closings.
            // Instead, we catch stream close when/if we next try to send.
            if let Ok(val) = recv_tcp_msg(&mut stream) {
                let msg: TcpRemoteNewMessage = val
                    .try_into()
                    .expect("Illegal message received from nn - shutting down.");

                self.sender
                    .send_buf_with_flags(msg.tag, msg.flags | LLMP_FLAG_FROM_NN, &msg.payload)
                    .expect("B2B: Error forwarding message. Exiting.");
            }
        } // end loop
    }
}

/*
* Helper functions
*/
async fn send_tcp_message<T>(stream: &mut TcpStream, msg: &T) -> Result<(), Error>
where
    T: Serialize,
{
    let msg = postcard::to_allocvec(msg)?;
    if msg.len() > u32::MAX as usize {
        return Err(Error::illegal_state(format!(
            "Trying to send a tcp message > u32 (size: {})",
            msg.len()
        )));
    }

    let size_bytes = (msg.len() as u32).to_be_bytes();
    stream.write_all(&size_bytes).await?;
    stream.write_all(&msg).await?;

    Ok(())
}

fn transform_stream(stream: TcpStream) -> Result<StdTcpStream, std::io::Error> {
    let mut std_tcp_stream = stream.into_std()?;
    std_tcp_stream.set_nonblocking(false)?;
    Ok(std_tcp_stream)
}

fn send_tcp_msg<T>(stream: &mut StdTcpStream, msg: &T) -> Result<(), Error>
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

async fn recv_tcp_message(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
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

/// Receive one message of `u32` len and `[u8; len]` bytes
fn recv_tcp_msg(stream: &mut StdTcpStream) -> Result<Vec<u8>, Error> {
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
