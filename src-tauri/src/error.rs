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

    /// Rewrites a raw driver error from a CONNECTION attempt into a plain,
    /// actionable sentence with a consistent `connectionFailed` kind, keeping the
    /// original text as `detail`. Only call this on the connect path — a real
    /// query error must keep the database's own message. Fixes the "timed out"
    /// (really: refused), DNS-jargon, and tiberius `Token error … code/state`
    /// noise a newcomer hits on a bad connection.
    pub fn humanize_connect(self) -> Self {
        let raw = self.message.clone();
        let low = raw.to_lowercase();
        let msg = if low.contains("login failed")
            || low.contains("access denied")
            || low.contains("password authentication failed")
            || low.contains("28p01")
        {
            "Login failed — check the username and password."
        } else if low.contains("cannot open database")
            || low.contains("unknown database")
            || (low.contains("database") && low.contains("does not exist"))
        {
            "That database doesn't exist, or this user can't access it — check the database name."
        } else if low.contains("refused")
            || low.contains("os error 61")
            || low.contains("connection reset")
        {
            "Can't reach the server — check the host and port, and that the server is running."
        } else if low.contains("lookup address")
            || low.contains("nodename nor servname")
            || low.contains("name or service not known")
            || low.contains("failed to lookup")
            || low.contains("no such host")
        {
            "Can't find that host — check the host name."
        } else if low.contains("timed out") || low.contains("timeout") {
            "The server didn't respond in time — check the host and port, and that the server is reachable."
        } else {
            return self; // unrecognized — leave the original message intact
        };
        AppError { kind: ErrorKind::ConnectionFailed, message: msg.to_string(), detail: Some(raw) }
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

    #[test]
    fn humanize_connect_rewrites_common_failures() {
        // tiberius login-failed noise → plain sentence, original kept as detail
        let e = AppError::query(
            "Token error: 'Login failed for user 'sa'.' on server abc executing on line 1 (code: 18456, state: 1, class: 14)",
        ).humanize_connect();
        assert!(matches!(e.kind, ErrorKind::ConnectionFailed));
        assert_eq!(e.message, "Login failed — check the username and password.");
        assert!(e.detail.unwrap().contains("18456"));

        assert_eq!(
            AppError::query("Cannot open database \"nope\" requested by the login.").humanize_connect().message,
            "That database doesn't exist, or this user can't access it — check the database name.",
        );
        assert!(AppError::internal("Connection refused (os error 61)").humanize_connect().message.starts_with("Can't reach"));
        assert!(AppError::connection("failed to lookup address information: nodename nor servname provided").humanize_connect().message.starts_with("Can't find that host"));
        // an unrecognized message (e.g. a genuine SQL error) is left intact
        let untouched = AppError::query("syntax error near 'FROM'").humanize_connect();
        assert_eq!(untouched.message, "syntax error near 'FROM'");
        assert!(matches!(untouched.kind, ErrorKind::QueryError));
    }
}
