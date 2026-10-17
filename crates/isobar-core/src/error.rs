use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("could not reach the station: {0}")]
    Http(String),
    #[error("the station replied with HTTP {0} for {1}")]
    Status(u16, String),
    #[error("unexpected data from the station: {0}")]
    Parse(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<reqwest::Error> for CoreError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            CoreError::Http("timed out".into())
        } else if e.is_connect() {
            CoreError::Http("connection refused or host not found".into())
        } else {
            CoreError::Http(e.to_string())
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
