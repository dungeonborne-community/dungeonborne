//! One error type for every command. `code` is a stable key the UI translates (ru/en); `detail` is
//! technical text for the log. Never put tokens into either field.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: String,
    pub detail: String,
    /// Server-provided https link the UI may offer (invite for `api.not_in_guild`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { code: code.into(), detail: detail.into(), link: None }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.detail.is_empty() {
            f.write_str(&self.code)
        } else {
            write!(f, "{}: {}", self.code, self.detail)
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new("io", e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new("json", e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors carry the URL; URLs we call never contain tokens (only login_id).
        Error::new("network", e.to_string())
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::new("mod_archive", e.to_string())
    }
}

macro_rules! bail {
    ($code:expr) => { return Err($crate::error::Error::new($code, "")) };
    ($code:expr, $($arg:tt)+) => { return Err($crate::error::Error::new($code, format!($($arg)+))) };
}
pub(crate) use bail;
