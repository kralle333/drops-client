use reqwest::StatusCode;
use std::io;

#[derive(Debug, Clone)]
pub enum LoginError {
    APIError,
    Unreachable,
    NotFound,
    MissingSessionToken,
    BadCredentials,
}

impl From<reqwest::Error> for LoginError {
    fn from(error: reqwest::Error) -> LoginError {
        match error.status() {
            Some(StatusCode::UNAUTHORIZED) => crate::LoginError::BadCredentials,
            Some(StatusCode::NOT_FOUND) => crate::LoginError::NotFound,
            None => crate::LoginError::Unreachable,
            _ => crate::LoginError::APIError,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FetchGamesError {
    APIError(String),
    Unreachable(String),
    NotFound,
    BadCredentials,
    NeedRelogin,
}

impl From<reqwest::Error> for crate::FetchGamesError {
    fn from(error: reqwest::Error) -> crate::FetchGamesError {
        match error.status() {
            Some(StatusCode::UNAUTHORIZED) => FetchGamesError::BadCredentials,
            Some(StatusCode::NOT_FOUND) => FetchGamesError::NotFound,
            Some(e) => FetchGamesError::APIError(format!("code: {}", e)),
            None => FetchGamesError::Unreachable(format!("{:?}", error)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigError {
    DialogClosed,
    IoError(io::ErrorKind),
}
