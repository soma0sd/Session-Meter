use thiserror::Error;

/// Application-level error. Commands convert this to a `String` for IPC.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("session expired")]
    Unauthorized,
    #[error("not signed in")]
    NoSession,
    #[error("Antigravity IDE not running")]
    NotRunning,
    #[error("network error: {0}")]
    Http(String),
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("{0}")]
    Other(String),
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}
