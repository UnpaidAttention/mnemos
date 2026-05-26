use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Server { status: u16, body: String },
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T, E = ClientError> = std::result::Result<T, E>;
