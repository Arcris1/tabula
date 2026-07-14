//! Local development service management (EnvKit-style). M1: database engines.
//! Manages system-installed services via the platform's service manager
//! (Homebrew `brew services` on macOS, Windows Services on Windows) and reports
//! live status via a TCP port probe.

use crate::error::AppError;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    /// stable id used for start/stop actions (brew formula / windows service name)
    pub id: String,
    /// engine kind for icon: nginx | apache | php | node | mysql | ... | redis
    pub kind: String,
    /// grouping: web | database | runtime
    pub category: String,
    pub name: String,
    pub description: String,
    pub running: bool,
    pub installed: bool,
    /// start/stop available (a service manager backend was found for it)
    pub manageable: bool,
    /// "brew" | "winservice" | "unmanaged"
    pub manager: String,
    pub version: Option<String>,
    pub port: Option<u16>,
}

struct Known {
    kind: &'static str,
    category: &'static str,
    name: &'static str,
    description: &'static str,
    /// 0 = not a port service (runtime); skip the port probe
    port: u16,
    version_bin: &'static str,
    version_arg: &'static str,
    /// brew formula names (also matches `name@version` variants)
    brew: &'static [&'static str],
    /// windows service name prefixes
    winsvc: &'static [&'static str],
}

const KNOWN: &[Known] = &[
    // web stack
    Known { kind: "nginx", category: "web", name: "Nginx", description: "Web server with SSL vhosts", port: 8080, version_bin: "nginx", version_arg: "-v", brew: &["nginx"], winsvc: &["nginx"] },
    Known { kind: "apache", category: "web", name: "Apache", description: "Apache HTTP server (httpd)", port: 8080, version_bin: "httpd", version_arg: "-v", brew: &["httpd"], winsvc: &["Apache"] },
    Known { kind: "php", category: "web", name: "PHP", description: "PHP-FPM runtime", port: 9000, version_bin: "php", version_arg: "--version", brew: &["php"], winsvc: &["php"] },
    // databases
    Known { kind: "mysql", category: "database", name: "MySQL", description: "MySQL database server", port: 3306, version_bin: "mysqld", version_arg: "--version", brew: &["mysql"], winsvc: &["MySQL"] },
    Known { kind: "mariadb", category: "database", name: "MariaDB", description: "MySQL-compatible database", port: 3307, version_bin: "mariadbd", version_arg: "--version", brew: &["mariadb"], winsvc: &["MariaDB"] },
    Known { kind: "postgres", category: "database", name: "PostgreSQL", description: "PostgreSQL database server", port: 5432, version_bin: "postgres", version_arg: "--version", brew: &["postgresql"], winsvc: &["postgresql"] },
    Known { kind: "redis", category: "database", name: "Redis", description: "In-memory cache and queues", port: 6379, version_bin: "redis-server", version_arg: "--version", brew: &["redis"], winsvc: &["Redis"] },
    // runtimes (version-only)
    Known { kind: "node", category: "runtime", name: "Node.js", description: "JavaScript runtime", port: 0, version_bin: "node", version_arg: "--version", brew: &[], winsvc: &[] },
];

fn port_open(port: u16) -> bool {
    if port == 0 { return false; } // runtime, not a port service
    use std::net::{SocketAddr, TcpStream};
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok()
}

fn which(bin: &str) -> bool {
    let cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    std::process::Command::new(cmd)
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pull the first `X.Y[.Z]` token out of a `--version` string → "v5.7.24".
fn parse_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                if b[i] == b'.' { dots += 1; }
                i += 1;
            }
            if dots >= 1 {
                return Some(format!("v{}", s[start..i].trim_end_matches('.')));
            }
        } else {
            i += 1;
        }
    }
    None
}

fn binary_version(bin: &str, arg: &str) -> Option<String> {
    let out = std::process::Command::new(bin).arg(arg).output().ok()?;
    let text = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).to_string()
    } else {
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    parse_version(&text)
}

// ------------------------------- macOS ------------------------------------
#[cfg(target_os = "macos")]
fn brew_path() -> Option<String> {
    for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn brew_services() -> Vec<(String, String)> {
    let Some(brew) = brew_path() else { return vec![] };
    let Ok(out) = std::process::Command::new(brew).args(["services", "list", "--json"]).output() else {
        return vec![];
    };
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    json.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| Some((e.get("name")?.as_str()?.to_string(), e.get("status")?.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub fn list() -> Vec<ServiceInfo> {
    let brew = brew_services();
    KNOWN.iter().map(|k| {
        let matches: Vec<&(String, String)> = brew.iter()
            .filter(|(n, _)| k.brew.iter().any(|b| n == b || n.starts_with(&format!("{b}@"))))
            .collect();
        let chosen = matches.iter().find(|(_, s)| s == "started").or_else(|| matches.first()).copied();
        let running = chosen.map(|(_, s)| s == "started").unwrap_or(false) || port_open(k.port);
        let installed = !matches.is_empty() || which(k.version_bin);
        ServiceInfo {
            id: chosen.map(|(n, _)| n.clone()).unwrap_or_else(|| k.brew.first().copied().unwrap_or(k.kind).to_string()),
            kind: k.kind.into(), category: k.category.into(), name: k.name.into(), description: k.description.into(),
            running, installed,
            manageable: chosen.is_some(),
            manager: if chosen.is_some() { "brew".into() } else { "unmanaged".into() },
            version: if installed { binary_version(k.version_bin, k.version_arg) } else { None },
            port: if k.port == 0 { None } else { Some(k.port) },
        }
    }).collect()
}

#[cfg(target_os = "macos")]
pub fn action(id: &str, action: &str) -> Result<(), AppError> {
    if !matches!(action, "start" | "stop" | "restart") {
        return Err(AppError::internal("unknown action"));
    }
    let brew = brew_path().ok_or_else(|| AppError::internal("Homebrew not found"))?;
    let out = std::process::Command::new(brew).args(["services", action, id]).output()?;
    if !out.status.success() {
        return Err(AppError::internal(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(())
}

// ------------------------------ Windows -----------------------------------
#[cfg(target_os = "windows")]
fn win_services() -> Vec<(String, String)> {
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "Get-Service | Select-Object Name,@{n='Status';e={$_.Status.ToString()}} | ConvertTo-Json -Compress"])
        .output();
    let Ok(out) = out else { return vec![] };
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    let arr = match &json {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(_) => vec![json.clone()],
        _ => vec![],
    };
    arr.iter()
        .filter_map(|e| Some((e.get("Name")?.as_str()?.to_string(), e.get("Status")?.as_str()?.to_string())))
        .collect()
}

#[cfg(target_os = "windows")]
pub fn list() -> Vec<ServiceInfo> {
    let svcs = win_services();
    KNOWN.iter().map(|k| {
        let matched = svcs.iter().find(|(n, _)| k.winsvc.iter().any(|w| n.to_lowercase().starts_with(&w.to_lowercase())));
        let running = matched.map(|(_, s)| s == "Running").unwrap_or(false) || port_open(k.port);
        let installed = matched.is_some() || which(k.version_bin);
        ServiceInfo {
            id: matched.map(|(n, _)| n.clone()).unwrap_or_else(|| k.winsvc.first().copied().unwrap_or(k.kind).to_string()),
            kind: k.kind.into(), category: k.category.into(), name: k.name.into(), description: k.description.into(),
            running, installed,
            manageable: matched.is_some(),
            manager: if matched.is_some() { "winservice".into() } else { "unmanaged".into() },
            version: if installed { binary_version(k.version_bin, k.version_arg) } else { None },
            port: if k.port == 0 { None } else { Some(k.port) },
        }
    }).collect()
}

#[cfg(target_os = "windows")]
pub fn action(id: &str, action: &str) -> Result<(), AppError> {
    let verb = match action {
        "start" => "Start-Service",
        "stop" => "Stop-Service",
        "restart" => "Restart-Service",
        _ => return Err(AppError::internal("unknown action")),
    };
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!("{verb} -Name '{id}'")])
        .output()?;
    if !out.status.success() {
        return Err(AppError::internal(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(())
}

// -------------------------- other (Linux, etc.) ---------------------------
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn list() -> Vec<ServiceInfo> {
    KNOWN.iter().map(|k| {
        let installed = which(k.version_bin);
        ServiceInfo {
            id: k.brew.first().copied().unwrap_or(k.kind).into(),
            kind: k.kind.into(), category: k.category.into(), name: k.name.into(), description: k.description.into(),
            running: port_open(k.port), installed,
            manageable: false, manager: "unmanaged".into(),
            version: if installed { binary_version(k.version_bin, k.version_arg) } else { None },
            port: if k.port == 0 { None } else { Some(k.port) },
        }
    }).collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn action(_id: &str, _action: &str) -> Result<(), AppError> {
    Err(AppError::internal("service management not supported on this platform yet"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_version() {
        assert_eq!(parse_version("mysqld  Ver 5.7.24 for osx11.1"), Some("v5.7.24".into()));
        assert_eq!(parse_version("postgres (PostgreSQL) 17.7 (Homebrew)"), Some("v17.7".into()));
        assert_eq!(parse_version("Redis server v=8.8.0 sha=0"), Some("v8.8.0".into()));
        assert_eq!(parse_version("no numbers here"), None);
    }

    #[test]
    fn known_services_cover_db_engines() {
        let kinds: Vec<_> = KNOWN.iter().map(|k| k.kind).collect();
        assert!(kinds.contains(&"mysql") && kinds.contains(&"postgres") && kinds.contains(&"redis"));
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn print_detected() {
        for s in super::list() {
            println!("{:10} running={:5} manageable={:5} manager={:10} version={:?} id={}",
                s.kind, s.running, s.manageable, s.manager, s.version, s.id);
        }
    }
}
