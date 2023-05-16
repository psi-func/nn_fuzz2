use libafl::prelude::{CorpusId, Flags};

use serde::{Deserialize, Serialize};
use crate::error::Error;

/// Messages to init connection
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TcpResponce {
    /// After receiving new connection sending hello from fuzzer  
    Hello {
        /// Fuzzer name
        name: String,
    },
    /// Notify the client that it has been accepted
    Accepted {},
    /// something went wrong, start from scratch?
    Error {
        /// Error description
        description: String,
    }
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


/// Messages to init connection
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TcpRequest {
    Hello {
        /// Client name
        name: String,
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

/// Enum describes reinforcement learning protocol
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RLProtoMessage {
    /// Predict heatmap for this input
    Predict {
        id: CorpusId,
        input: Vec<u8>,
        map: Vec<u8>,
    },
    /// NN prediction with hotbytes indexes
    HeatMap {
        id: CorpusId,
        idxs: Vec<u32>,
    },
    // Coverage map after each mutaton with hotbytes
    MapAfterMutation {
        id: CorpusId,
        input: Vec<u8>,
        map: Vec<u8>
    },
    /// Reward for NN prediction
    Reward {
        id: CorpusId,
        score: f64,
    },
    /// Error message
    Error(String),
}

impl TryFrom<Vec<u8>> for RLProtoMessage {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
    }
}

impl TryFrom<&Vec<u8>> for RLProtoMessage {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes.as_slice()).map_err(Error::from)
    }
}

pub const COMPRESSION_THRESHOLD : usize = 0x1000;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TcpNewMessage {
    /// the flags
    pub flags: Flags,
    /// actual context of message
    pub payload: Vec<u8>,
}


impl TryFrom<&Vec<u8>> for TcpNewMessage {
    type Error = Error;

    fn try_from(bytes: &Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(bytes).map_err(Error::from)
    }
}

impl TryFrom<Vec<u8>> for TcpNewMessage {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        postcard::from_bytes(&bytes).map_err(Error::from)
    }
}