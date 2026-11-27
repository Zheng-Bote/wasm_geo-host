//! In-process HTTP client of the `http` capability (ADR 0007). All servers are local.

mod common;

use flagme_host::capabilities::{HTTP_MAX_RESPONSE_BYTES, HTTP_USER_AGENT};
use flagme_host::CapabilityManager;
use flagme_sdk::{HostCapabilities, PluginError};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

fn manager() -> CapabilityManager {
    CapabilityManager::new("test", vec!["http".to_string()], Arc::new(RwLock::new(HashMap::new())))
}

/// Reads the request line and headers; returns the path and the `User-Agent`.
fn read_request(stream: &mut impl Read) -> (String, String) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut agent = String::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).map(|n| n <= 2).unwrap_or(true) {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("user-agent:") {
            agent = v.trim().to_string();
        }
    }
    (path, agent)
}

fn respond(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
    let _ = write!(stream, "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n", status, body.len(), headers);
    let _ = stream.write_all(body);
}

/// Local server: `/ok`, `/missing`, `/broken`, `/empty`, `/binary`, `/exact` (1 MiB),
/// `/over` (1 MiB + 1), `/endless` (never-ending chunked body), `/r/<n>` (n redirects, then 200),
/// `/ftp` (redirect to ftp://), `/agent` (echoes the User-Agent), `/hang` (never answers).
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let (path, agent) = read_request(&mut stream.try_clone().unwrap());
                let limit = HTTP_MAX_RESPONSE_BYTES as usize;
                match path.as_str() {
                    "/ok" => respond(&mut stream, "200 OK", "", b"real body"),
                    "/missing" => respond(&mut stream, "404 Not Found", "", b"no such page"),
                    "/broken" => respond(&mut stream, "500 Internal Server Error", "", b"server error"),
                    "/empty" => respond(&mut stream, "200 OK", "", b""),
                    "/binary" => respond(&mut stream, "200 OK", "", &[0xff, 0xfe, 0x00, 0x41]),
                    "/exact" => respond(&mut stream, "200 OK", "", &vec![b'a'; limit]),
                    "/over" => respond(&mut stream, "200 OK", "", &vec![b'a'; limit + 1]),
                    "/endless" => {
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
                        let chunk = [b'x'; 16 * 1024];
                        loop {
                            if write!(stream, "{:x}\r\n", chunk.len()).is_err()
                                || stream.write_all(&chunk).is_err()
                                || stream.write_all(b"\r\n").is_err()
                            {
                                break;
                            }
                        }
                    }
                    "/ftp" => respond(&mut stream, "302 Found", "Location: ftp://127.0.0.1/file\r\n", b""),
                    "/agent" => respond(&mut stream, "200 OK", "", agent.as_bytes()),
                    "/hang" => std::thread::sleep(Duration::from_secs(10)),
                    p if p.starts_with("/r/") => {
                        let n: u32 = p[3..].parse().unwrap_or(0);
                        if n == 0 {
                            respond(&mut stream, "200 OK", "", b"arrived");
                        } else {
                            respond(&mut stream, "302 Found", &format!("Location: /r/{}\r\n", n - 1), b"");
                        }
                    }
                    _ => respond(&mut stream, "404 Not Found", "", b"?"),
                }
            });
        }
    });
    base
}

fn io_message(err: PluginError) -> String {
    match err {
        PluginError::Io(m) => m,
        other => panic!("expected PluginError::Io, got {:?}", other),
    }
}

// http-client-rust AC-1: the host code no longer starts curl
#[test]
fn test_ac1_no_curl_in_host() {
    let src = common::repo_root().join("crates/host/src");
    for entry in std::fs::read_dir(&src).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        assert!(!text.contains("Command::new(\"curl\")"), "{} starts curl", entry.path().display());
    }
}

// http-client-rust AC-2: 200 returns the real body; 404/500, empty, non-UTF-8 and unreachable fail with the cause; every message starts with "http_get <url>:" and does not mention curl
#[test]
fn test_ac2_error_contract() {
    let base = serve();
    assert_eq!(manager().http_get(&format!("{}/ok", base)).unwrap(), "real body");
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        drop(l);
        format!("http://{}/nothing", a)
    };
    for (url, cause) in [
        (format!("{}/missing", base), "HTTP status 404"),
        (format!("{}/broken", base), "HTTP status 500"),
        (format!("{}/empty", base), "empty response"),
        (format!("{}/binary", base), "response is not UTF-8"),
        (closed, "Connection refused"),
    ] {
        let m = io_message(manager().http_get(&url).unwrap_err());
        assert!(m.starts_with(&format!("http_get {}: ", url)), "{}", m);
        assert!(m.contains(cause), "{}: {}", url, m);
        assert!(!m.to_lowercase().contains("curl"), "{}", m);
    }
}

// http-client-rust AC-3: a response over 1 MiB fails naming the limit, also for an endless stream before the timeout; exactly 1 MiB succeeds
#[test]
fn test_ac3_response_size_limit() {
    let base = serve();
    assert_eq!(manager().http_get(&format!("{}/exact", base)).unwrap().len(), HTTP_MAX_RESPONSE_BYTES as usize);
    let m = io_message(manager().http_get(&format!("{}/over", base)).unwrap_err());
    assert!(m.contains("response larger than 1048576 bytes"), "{}", m);

    let start = Instant::now();
    let m = io_message(manager().http_get(&format!("{}/endless", base)).unwrap_err());
    assert!(m.contains("response larger than 1048576 bytes"), "{}", m);
    assert!(start.elapsed() < Duration::from_millis(1900), "stopped by the timeout, not the limit: {:?}", start.elapsed());
}

// http-client-rust AC-4: 5 redirects succeed; 6 redirects and a redirect to a non-http(s) scheme fail
#[test]
fn test_ac4_redirect_limit() {
    let base = serve();
    assert_eq!(manager().http_get(&format!("{}/r/5", base)).unwrap(), "arrived");
    let m = io_message(manager().http_get(&format!("{}/r/6", base)).unwrap_err());
    assert!(m.contains("more than 5 redirects"), "{}", m);
    let m = io_message(manager().http_get(&format!("{}/ftp", base)).unwrap_err());
    assert!(m.starts_with(&format!("http_get {}/ftp: ", base)), "{}", m);
}

// http-client-rust AC-5: every request carries User-Agent flagme-host/<version>; a server that never answers fails with a timeout after at most 2 s (tolerance 3 s)
#[test]
fn test_ac5_user_agent_and_timeout() {
    let base = serve();
    assert_eq!(manager().http_get(&format!("{}/agent", base)).unwrap(), HTTP_USER_AGENT.to_lowercase());
    assert_eq!(HTTP_USER_AGENT, concat!("flagme-host/", env!("CARGO_PKG_VERSION")));

    let start = Instant::now();
    let m = io_message(manager().http_get(&format!("{}/hang", base)).unwrap_err());
    let elapsed = start.elapsed();
    assert!(m.contains("timed out after 2 s"), "{}", m);
    assert!(elapsed >= Duration::from_millis(1900) && elapsed <= Duration::from_secs(3), "{:?}", elapsed);
}

/// Local HTTPS server with a freshly generated self-signed certificate for `localhost`.
fn serve_self_signed() -> String {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(tcp) = stream else { continue };
            let conn = rustls::ServerConnection::new(config.clone()).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, tcp);
            let mut buf = [0u8; 1024];
            if tls.read(&mut buf).is_ok() {
                let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret");
            }
        }
    });
    format!("https://localhost:{}/", port)
}

// http-client-rust AC-6: an HTTPS server with a self-signed certificate makes the request fail with a certificate error (local, no network)
#[test]
fn test_ac6_self_signed_certificate_rejected() {
    let url = serve_self_signed();
    let m = io_message(manager().http_get(&url).unwrap_err());
    assert!(m.starts_with(&format!("http_get {}: ", url)), "{}", m);
    assert!(m.to_lowercase().contains("certificate"), "{}", m);
    assert!(!m.contains("secret"), "{}", m);
}

// http-client-rust AC-9: deployment guide, arc42 and CHANGELOG no longer list curl as a runtime requirement and describe system certificates, proxy variables and the fixed limits
#[test]
fn test_ac9_docs_without_curl_runtime() {
    let root = common::repo_root();
    let guide = std::fs::read_to_string(root.join("docs/deployment-guide.md")).unwrap();
    let arc42 = std::fs::read_to_string(root.join("docs/arc42.md")).unwrap();
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    for (name, text) in [("deployment guide", &guide), ("arc42", &arc42)] {
        assert!(!text.contains("`curl`"), "{} still mentions curl as a runtime program", name);
    }
    for needle in ["ca-certificates", "HTTPS_PROXY", "NO_PROXY", "1 MiB", "5 redirects", "2 s"] {
        assert!(guide.contains(needle), "deployment guide lacks {}", needle);
    }
    assert!(arc42.contains("ureq"), "arc42 lacks the HTTP client");
    assert!(changelog.contains("ADR 0007") && changelog.contains("ureq"), "CHANGELOG lacks the change");
}
