use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot::Sender;

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::net::TcpStream as StdTcpStream;
use std::time::Duration;

use libafl::bolts::llmp::{ClientId, LlmpDescription, LlmpReceiver, LlmpSender};
use libafl::bolts::shmem::{ShMem, ShMemDescription, ShMemProvider};
use libafl::Error;

use serde::{Deserialize, Serialize};

use super::messages::{TcpRequest, TcpResponce, TcpRemoteNewMessage, FuzzerDescription, LLMP_FLAG_FROM_NN};

const _MAX_WORKING_THREADS: usize = 2;
const _LLMP_NN_BLOCK_TIME: Duration = Duration::from_millis(3_000);

#[cfg(feature = "bind_public")]
const _BIND_ADDR: &str = "0.0.0.0";

#[cfg(not(feature = "bind_public"))]
const _BIND_ADDR: &str = "127.0.0.1";

#[derive(Debug)]
enum Listener {
    Tcp(TcpListener),
}

impl Listener {
    async fn accept(&self) -> ListenerStream {
        match self {
            Listener::Tcp(inner) => match inner.accept().await {
                Ok(res) => ListenerStream::Tcp(res.0, res.1),
                Err(_) => ListenerStream::Empty,
            },
        }
    }
}

#[derive(Debug)]
pub enum ListenerStream {
    Tcp(TcpStream, SocketAddr),
    Empty,
}

/// Info which NN provides before start
#[derive(Serialize, Deserialize, Debug, Clone)]
struct NNDescription {
    nn_name: String,
    nn_version: String,
}

/// 
/// # Panics
///    panics if port is already used bu other process
///
pub async fn run_service<SP: ShMemProvider + 'static>(
    sender: Sender<ShMemDescription>,
    broker_shmem_description: ShMemDescription,
    _client_id: ClientId,
    port: u16,
) {
    let listener = Listener::Tcp(
        TcpListener::bind((_BIND_ADDR, port))
            .await
            .unwrap_or_else(|_| panic!("NN connector: Cannot bind to port: {port}")),
    );

    let hello = TcpResponce::RemoteFuzzerHello {
        fuzz_description: FuzzerDescription {
            ec_size: crate::MAP_SIZE,
            // TODO: get real values
            instances: 0,
            fuzz_target: String::new(),
        },
    };

    let mut sender = Some(sender);

    loop {
        match listener.accept().await {
            ListenerStream::Tcp(mut stream, _) => {
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
                        eprintln!("Could not deserialize tcp message: {e:?}");
                        continue;
                    }
                };

                // handle connection
                match req {
                    // remote nn connection
                    TcpRequest::RemoteNnHello {
                        nn_name,
                        nn_version,
                    } => {
                        if let Some(send) = sender.take() {
                            let client_id = u32::MAX;

                            let msg = TcpResponce::RemoteNNAccepted { client_id };

                            if let Err(_e) = send_tcp_message(&mut stream, &msg).await {
                                println!("Cannot send message");
                            }

                            tokio::task::spawn_blocking(move || {
                                let mut nn_connector: NnConnector<SP> =
                                    NnConnector::new(broker_shmem_description, client_id, send);

                                let description = NNDescription {
                                    nn_name,
                                    nn_version,
                                };

                                nn_connector.handle_connection(stream, &description);
                            });
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
    _id: ClientId,
}

impl InstanceConnector {
    fn new(client_id: ClientId) -> Self {
        InstanceConnector { _id: client_id }
    }

    async fn handle_connection(&mut self, _stream: TcpStream) {
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

        let new_sender = match LlmpSender::new(shmem_provider_bg.clone(), client_id, false) {
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

    fn handle_connection(&mut self, stream: TcpStream, _desc: &NNDescription) {
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
    if let Ok(len) = u32::try_from(msg.len()) {
        let size_bytes = len.to_be_bytes();
        stream.write_all(&size_bytes).await?;
        stream.write_all(&msg).await?;
        Ok(())
    } else {
        Err(Error::illegal_state(format!(
            "Trying to send a tcp message > u32 (size: {})",
            msg.len()
        )))
    }
}

fn transform_stream(stream: TcpStream) -> Result<StdTcpStream, std::io::Error> {
    let std_tcp_stream = stream.into_std()?;
    std_tcp_stream.set_nonblocking(false)?;
    Ok(std_tcp_stream)
}

fn send_tcp_msg<T>(stream: &mut StdTcpStream, msg: &T) -> Result<(), Error>
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
        return Err(Error::illegal_state(format!(
            "Trying to send message a tcp message > u32! (size: {})",
            msg.len()
        )));
    }
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
