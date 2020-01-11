use err_derive::Error;

use std::io::Error as IoError;
use http::Error as HttpError;
use hyper::Error as HyperError;
use quinn_proto::ConnectionError;
use quinn::{ReadError, ReadToEndError, WriteError, EndpointError};

pub type Result<T> = std::result::Result<T, ProxyError>;

/// Proxy error
#[derive(Debug, Error)]
pub enum ProxyError {
    #[error(display = "Invalid configuration")]
    ConfigurationError,
    #[error(display = "Invalid request")]
    InvalidRequest,
    #[error(display = "IO Error {}", _0)]
    IoError(IoError),
    #[error(display = "Http Error {}", _0)]
    HttpError(HttpError),
    #[error(display = "Hyper Error {}", _0)]
    HyperError(HyperError),
    #[error(display = "Read Error {}", _0)]
    ReadError(ReadError),
    #[error(display = "ReadToEnd Error {}", _0)]
    ReadToEndError(ReadToEndError),
    #[error(display = "Write Error {}", _0)]
    WriteError(WriteError),
    #[error(display = "Endpoint Error {}", _0)]
    EndpointError(EndpointError),
    #[error(display = "Connection Error {}", _0)]
    ConnectionError(ConnectionError),
}

impl From<IoError> for ProxyError {
    fn from(err: IoError) -> Self {
        ProxyError::IoError(err)
    }
}

impl From<HttpError> for ProxyError {
    fn from(err: HttpError) -> Self {
        ProxyError::HttpError(err)
    }
}

impl From<HyperError> for ProxyError {
    fn from(err: HyperError) -> Self {
        ProxyError::HyperError(err)
    }
}

impl From<ReadError> for ProxyError {
    fn from(err: ReadError) -> Self {
        ProxyError::ReadError(err)
    }
}

impl From<ReadToEndError> for ProxyError {
    fn from(err: ReadToEndError) -> Self {
        ProxyError::ReadToEndError(err)
    }
}

impl From<WriteError> for ProxyError {
    fn from(err: WriteError) -> Self {
        ProxyError::WriteError(err)
    }
}

impl From<EndpointError> for ProxyError {
    fn from(err: EndpointError) -> Self {
        ProxyError::EndpointError(err)
    }
}

impl From<ConnectionError> for ProxyError {
    fn from(err: ConnectionError) -> Self {
        ProxyError::ConnectionError(err)
    }
}