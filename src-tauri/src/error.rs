use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),
    #[error("network: {0}")]
    Http(#[from] reqwest::Error),
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not logged in")]
    NotLoggedIn,
    #[error("rate limited for {0}s")]
    RateLimited(u64),
    #[error("no active device")]
    NoDevice,
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Msg(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

pub fn msg(s: impl Into<String>) -> AppError {
    AppError::Msg(s.into())
}
