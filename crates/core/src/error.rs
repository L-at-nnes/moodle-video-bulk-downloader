use std::fmt;

#[derive(Debug)]
pub struct MvbdError(pub String);

impl fmt::Display for MvbdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MvbdError {}

impl MvbdError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl From<std::io::Error> for MvbdError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<reqwest::Error> for MvbdError {
    fn from(e: reqwest::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<chromiumoxide::error::CdpError> for MvbdError {
    fn from(e: chromiumoxide::error::CdpError) -> Self {
        Self(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, MvbdError>;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::MvbdError::new(format!($($arg)*)))
    };
}
