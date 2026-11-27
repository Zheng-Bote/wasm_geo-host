//! Separate test binary: it changes `PATH` and the proxy variables for the whole process, which
//! must not affect other tests (ADR 0007).

use flagme_host::CapabilityManager;
use flagme_sdk::{HostCapabilities, PluginError};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};

fn manager() -> CapabilityManager {
    CapabilityManager::new("test", vec!["http".to_string()], Arc::new(RwLock::new(HashMap::new())))
}

/// Server that answers every request with `body` and reports the request line. A `CONNECT`
/// (how ureq tunnels through a proxy) is acknowledged and the tunnelled request answered.
fn serve(body: &'static str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let _ = tx.send(line.trim().to_string());
            let mut h = String::new();
            while reader.read_line(&mut h).map(|n| n > 2).unwrap_or(false) {
                h.clear();
            }
            if line.starts_with("CONNECT ") {
                let _ = write!(stream, "HTTP/1.1 200 Connection established\r\n\r\n");
                let mut inner = String::new();
                while reader.read_line(&mut inner).map(|n| n > 2).unwrap_or(false) {
                    inner.clear();
                }
            }
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    (addr, rx)
}

// http-client-rust AC-1: http_get works with an empty PATH (no curl needed); an unreachable server still fails with the cause
// http-client-rust AC-7: with HTTP_PROXY set an http:// request goes through the proxy; a host in NO_PROXY is reached directly
#[test]
fn test_ac1_ac7_environment() {
    // One test function: both parts change process-wide environment variables.
    let (origin, origin_requests) = serve("from origin");
    let (proxy, proxy_requests) = serve("from proxy");

    std::env::set_var("PATH", "");
    for var in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"] {
        std::env::remove_var(var);
    }
    assert_eq!(manager().http_get(&format!("http://{}/direct", origin)).unwrap(), "from origin");
    assert!(origin_requests.recv().unwrap().starts_with("GET /direct "));
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        drop(l);
        format!("http://{}/x", a)
    };
    let err = manager().http_get(&closed).unwrap_err();
    assert!(matches!(&err, PluginError::Io(m) if m.starts_with(&format!("http_get {}: ", closed))), "{:?}", err);

    // The origin is addressed by a name that is not in NO_PROXY, so the request must go to
    // the proxy, which ureq asks for a tunnel to the origin (CONNECT host:port).
    std::env::set_var("HTTP_PROXY", format!("http://{}", proxy));
    std::env::set_var("NO_PROXY", "localhost");
    let target = format!("http://{}/via-proxy", origin);
    assert_eq!(manager().http_get(&target).unwrap(), "from proxy");
    let line = proxy_requests.recv().unwrap();
    assert!(line.starts_with(&format!("CONNECT {} ", origin)), "{}", line);
    assert!(origin_requests.try_recv().is_err(), "the origin must not be contacted directly");

    // localhost is in NO_PROXY: direct connection to the origin.
    let port = origin.rsplit(':').next().unwrap();
    assert_eq!(manager().http_get(&format!("http://localhost:{}/bypass", port)).unwrap(), "from origin");
    assert!(origin_requests.recv().unwrap().starts_with("GET /bypass "));
}
