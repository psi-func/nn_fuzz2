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
        input: Vec<u8>,
    },
    /// NN prediction with hotbytes indexes
    HeatMap {
        idxs: Vec<u32>,
    },
    /// Reward for NN prediction
    Reward {
        score: f64,
    },
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