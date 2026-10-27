use std::fmt;

/// A failure that stops the server: a broken connection or malformed protocol data.
#[derive(Debug)]
pub struct LspError(String);

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LspError {}

impl From<lsp_server::ProtocolError> for LspError {
    fn from(error: lsp_server::ProtocolError) -> Self {
        LspError(format!("protocol error: {error}"))
    }
}

impl From<std::io::Error> for LspError {
    fn from(error: std::io::Error) -> Self {
        LspError(format!("i/o error: {error}"))
    }
}

impl From<serde_json::Error> for LspError {
    fn from(error: serde_json::Error) -> Self {
        LspError(format!("invalid JSON: {error}"))
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for LspError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        LspError("the client connection is closed".to_owned())
    }
}
