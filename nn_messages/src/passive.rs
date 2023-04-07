use serde::{Deserialize, Serialize};
use crate::error::Error;

use libafl::prelude::{ClientId, Flags, Tag};

/// Messages for nn connection.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TcpRemoteNewMessage {
    /// the client ID of the fuzzer
    pub client_id: ClientId,
    /// the message tag
    pub tag: Tag,
    /// the flags
    pub flags: Flags,
    /// actual content of message
    pub payload: Vec<u8>,
}

impl TryFrom<&Vec<u8>> for TcpRemoteNewMessage {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes).map_err(Error::from)
    }
}

impl TryFrom<Vec<u8>> for TcpRemoteNewMessage {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(&bytes).map_err(Error::from)
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
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
    }
}

impl TryFrom<Vec<u8>> for TcpResponce {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
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
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
    }
}

impl TryFrom<&Vec<u8>> for TcpRequest {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
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
