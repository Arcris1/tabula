use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    ConnectionFailed,
    TunnelFailed,
    QueryError,
    ConflictOnCommit,
    Timeout,
    Internal,
}

#[derive(Debug, Serialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), detail: None }
    }
    pub fn connection(m: impl Into<String>) -> Self { Self::new(ErrorKind::ConnectionFailed, m) }
    pub fn conflict(m: impl Into<String>) -> Self { Self::new(ErrorKind::ConflictOnCommit, m) }
    pub fn query(m: impl Into<String>) -> Self { Self::new(ErrorKind::QueryError, m) }
    pub fn timeout(m: impl Into<String>) -> Self { Self::new(ErrorKind::Timeout, m) }
    pub fn internal(m: impl Into<String>) -> Self { Self::new(ErrorKind::Internal, m) }
    pub fn with_detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::PoolTimedOut => AppError::timeout("database connection timed out"),
            sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::Configuration(_) => {
                AppError::connection(e.to_string())
            }
            sqlx::Error::Database(db) => AppError::query(db.message().to_string()),
            _ => AppError::query(e.to_string()),
        }
    }
}

impl From<redis::RedisError> for AppError {
    fn from(e: redis::RedisError) -> Self {
        if e.is_connection_refusal() || e.is_io_error() {
            AppError::connection(e.to_string())
        } else {
            AppError::query(e.to_string())
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::internal(e.to_string()) }
}

impl From<tiberius::error::Error> for AppError {
    fn from(e: tiberius::error::Error) -> Self {
        match &e {
            tiberius::error::Error::Io { .. } | tiberius::error::Error::Tls(_) => {
                AppError::connection(e.to_string())
            }
            _ => AppError::query(e.to_string()),
        }
    }
}

impl From<bb8_tiberius::Error> for AppError {
    fn from(e: bb8_tiberius::Error) -> Self { AppError::connection(e.to_string()) }
}

impl From<bb8::RunError<bb8_tiberius::Error>> for AppError {
    fn from(e: bb8::RunError<bb8_tiberius::Error>) -> Self {
        match e {
            bb8::RunError::User(e) => e.into(),
            bb8::RunError::TimedOut => AppError::timeout("database connection timed out"),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { AppError::internal(e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_message_detail() {
        let e = AppError::query("syntax error").with_detail("near SELECT");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "queryError");
        assert_eq!(v["message"], "syntax error");
        assert_eq!(v["detail"], "near SELECT");
    }

    #[test]
    fn maps_sqlx_error() {
        let e: AppError = sqlx::Error::PoolTimedOut.into();
        assert!(matches!(e.kind, ErrorKind::Timeout));
    }
}
