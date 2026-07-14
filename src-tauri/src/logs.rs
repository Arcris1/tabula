//! Live log tailing for the local-stack manager (M4). Detects known service
//! log files and reads incrementally from a byte offset so huge logs (e.g. a
//! multi-GB php-fpm.log) are never read whole.

use crate::error::AppError;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSource {
    pub id: String,
    pub label: String,
    pub category: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    pub content: String,
    /// byte offset to pass on the next read (current end of file)
    pub offset: u64,
    pub size: u64,
}

/// bytes of history to show on the first read / after a rotation
const TAIL: u64 = 64 * 1024;

pub fn read(path: &str, from: Option<u64>) -> Result<LogChunk, AppError> {
    let mut f = std::fs::File::open(path)
        .map_err(|e| AppError::internal(format!("open {path}: {e}")))?;
    let size = f.metadata()?.len();
    let start = match from {
        None => size.saturating_sub(TAIL),                       // first read: tail
        Some(o) if o <= size => {
            if size - o > TAIL { size - TAIL } else { o }         // cap a big catch-up
        }
        Some(_) => size.saturating_sub(TAIL),                    // truncated/rotated
    };
    f.seek(SeekFrom::Start(start))?;
    let to_read = (size - start).min(TAIL * 4);                  // hard safety cap
    let mut buf = vec![0u8; to_read as usize];
    let n = f.read(&mut buf)?;
    buf.truncate(n);
    Ok(LogChunk {
        content: String::from_utf8_lossy(&buf).into_owned(),
        offset: size,
        size,
    })
}

// --------------------------- detection (macOS) ----------------------------
#[cfg(target_os = "macos")]
fn brew_prefix() -> Option<String> {
    for p in ["/opt/homebrew", "/usr/local"] {
        if std::path::Path::new(&format!("{p}/bin/brew")).exists() {
            return Some(p.to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn list() -> Vec<LogSource> {
    let Some(p) = brew_prefix() else { return vec![] };
    let mut out: Vec<LogSource> = vec![];
    let mut add = |id: &str, label: &str, category: &str, path: String| {
        if std::path::Path::new(&path).is_file() {
            out.push(LogSource { id: id.into(), label: label.into(), category: category.into(), path });
        }
    };
    add("nginx-access", "Nginx · access.log", "web", format!("{p}/var/log/nginx/access.log"));
    add("nginx-error", "Nginx · error.log", "web", format!("{p}/var/log/nginx/error.log"));
    add("php-fpm", "PHP-FPM · php-fpm.log", "web", format!("{p}/var/log/php-fpm.log"));
    add("redis", "Redis · redis.log", "database", format!("{p}/var/log/redis.log"));
    add("mailpit", "Mailpit · mailpit.log", "tool", format!("{p}/var/log/mailpit.log"));

    // globs: postgres versioned logs, mysql per-host .err
    if let Ok(entries) = std::fs::read_dir(format!("{p}/var/log")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("postgresql@") && name.ends_with(".log") {
                out.push(LogSource {
                    id: format!("pg-{name}"), label: format!("PostgreSQL · {name}"),
                    category: "database".into(), path: e.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(format!("{p}/var/mysql")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".err") {
                out.push(LogSource {
                    id: format!("mysql-{name}"), label: format!("MySQL · {name}"),
                    category: "database".into(), path: e.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn list() -> Vec<LogSource> {
    // Windows/Linux log locations vary widely; users add a custom path.
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn incremental_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.log");
        std::fs::write(&path, b"line1\nline2\n").unwrap();
        let p = path.to_string_lossy().to_string();

        let first = read(&p, None).unwrap();
        assert!(first.content.contains("line1") && first.content.contains("line2"));
        assert_eq!(first.offset, 12);

        // append and read only the new bytes
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"line3\n").unwrap();
        let next = read(&p, Some(first.offset)).unwrap();
        assert_eq!(next.content, "line3\n");
        assert_eq!(next.offset, 18);

        // no change → empty
        let none = read(&p, Some(next.offset)).unwrap();
        assert_eq!(none.content, "");
    }
}
