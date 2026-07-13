use dbclient_lib::drivers::mysql::MysqlDriver;
use dbclient_lib::drivers::SqlDriver;
use dbclient_lib::tunnel::{SshAuth, SshTunnel};

#[tokio::test]
async fn tunnel_to_mysql_via_sshd() {
    if std::env::var("DBCLIENT_TEST_SSH").is_err() {
        eprintln!("skipping: DBCLIENT_TEST_SSH not set");
        return;
    }
    // first use records the host key
    let t = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("tunnelpass") },
        "mysql", 3306, // compose service name: only reachable through the tunnel
        None,
    ).await.unwrap();
    assert!(t.local_port > 0);
    let host_key = t.new_host_key.clone().expect("first use should record a host key");

    let d = MysqlDriver::connect("127.0.0.1", t.local_port, "root", "test", "dbclient_test").await.unwrap();
    d.ping().await.unwrap();

    // reconnecting with the recorded key matches (no new key to persist)
    let t2 = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("tunnelpass") },
        "mysql", 3306,
        Some(host_key.clone()),
    ).await.unwrap();
    assert!(t2.new_host_key.is_none(), "matched key must not be re-recorded");

    // a WRONG expected key => refuse (MITM detection)
    let err = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("tunnelpass") },
        "mysql", 3306,
        Some("SHA256:deadbeefwrongkey".into()),
    ).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::TunnelFailed);
    assert!(err.message.contains("host key"), "mismatch must be a host-key error: {}", err.message);

    // bad auth -> TunnelFailed
    let err = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("wrong") },
        "mysql", 3306,
        Some(host_key.clone()),
    ).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::TunnelFailed);

    // regression (db-app-explorer B4): a failed channel open (dead target) must
    // not kill the listener — the tunnel keeps accepting new connections
    let dead = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("tunnelpass") },
        "mysql", 9999, // closed port on the target host
        Some(host_key),
    ).await.unwrap();
    for attempt in 0..2 {
        let conn = tokio::net::TcpStream::connect(("127.0.0.1", dead.local_port)).await;
        assert!(conn.is_ok(), "listener must stay alive (attempt {attempt})");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
