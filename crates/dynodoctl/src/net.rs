//! What the operator's machine can see of an installation: a small blocking HTTP/1.0 client over
//! `std::net` and rustls. No async runtime, no redirects followed, no proxy settings, no cookies.
//!
//! Certificate trust is evaluated against the Mozilla roots in `webpki-roots` plus any `--ca-file`
//! authorities. The operating system's trust store is NOT consulted (it would need another
//! crate); a company authority is passed with `--ca-file`. The result is recorded, and the
//! connection continues either way, so an evaluation certificate can still be inspected.

use crate::x509;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, RootCertStore, SignatureScheme};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_BODY: usize = 6 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// Header names lowercased, in order, duplicates kept (a second CSP is a defect to report).
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn header_count(&self, name: &str) -> usize {
        let name = name.to_ascii_lowercase();
        self.headers.iter().filter(|(k, _)| *k == name).count()
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// What the TLS handshake showed.
#[derive(Clone, Debug)]
pub struct TlsFacts {
    pub protocol: String,
    pub cipher: String,
    /// DER certificates, server certificate first.
    pub chain: Vec<Vec<u8>>,
    /// Ok when the chain is valid for the server name against the trusted roots, else why not.
    pub trust: Result<(), String>,
}

impl Default for TlsFacts {
    fn default() -> Self {
        Self {
            protocol: String::new(),
            cipher: String::new(),
            chain: Vec::new(),
            trust: Err("no handshake".into()),
        }
    }
}

impl TlsFacts {
    pub fn leaf(&self) -> Option<x509::CertInfo> {
        self.chain.first().and_then(|der| x509::parse_cert(der))
    }
}

pub struct Target {
    /// Where to connect (name or address) and the port.
    pub connect_host: String,
    pub port: u16,
    /// The name in the certificate, in SNI and in the Host header (with the port if not default).
    pub server_name: String,
    pub host_header: String,
}

pub trait Net {
    fn https_get(&self, target: &Target, path: &str) -> Result<(HttpResponse, TlsFacts), String>;
    fn http_get(&self, target: &Target, path: &str) -> Result<HttpResponse, String>;
}

/// The certificate verifier is built once per run: parsing the ~150 built-in roots for every
/// request made a delivery check take half a minute in a debug build.
struct Shared {
    provider: Arc<rustls::crypto::CryptoProvider>,
    verifier: Arc<WebPkiServerVerifier>,
}

pub struct RealNet {
    pub extra_roots: Vec<Vec<u8>>,
    /// For one connect, read or write.
    pub timeout: Duration,
    /// For a whole request, handshake and body included.
    pub deadline: Duration,
    shared: std::sync::OnceLock<Result<Shared, String>>,
    /// The address that last accepted a connection, per `host:port`.
    good: Mutex<std::collections::HashMap<String, std::net::SocketAddr>>,
}

impl RealNet {
    pub fn new(extra_roots: Vec<Vec<u8>>) -> Self {
        Self {
            extra_roots,
            timeout: Duration::from_secs(10),
            deadline: Duration::from_secs(30),
            shared: std::sync::OnceLock::new(),
            good: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn shared(&self) -> Result<&Shared, String> {
        self.shared
            .get_or_init(|| {
                let provider = Arc::new(rustls::crypto::ring::default_provider());
                let mut roots = RootCertStore {
                    roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
                };
                for der in &self.extra_roots {
                    roots
                        .add(CertificateDer::from(der.clone()))
                        .map_err(|e| format!("a --ca-file certificate is not usable: {e}"))?;
                }
                let verifier =
                    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                        .build()
                        .map_err(|e| format!("cannot build the certificate verifier: {e}"))?;
                Ok(Shared { provider, verifier })
            })
            .as_ref()
            .map_err(|e| e.clone())
    }

    fn connect(&self, target: &Target) -> Result<TcpStream, String> {
        let key = format!("{}:{}", target.connect_host, target.port);
        let mut addresses: Vec<_> = (target.connect_host.as_str(), target.port)
            .to_socket_addrs()
            .map_err(|e| format!("cannot resolve {}: {e}", target.connect_host))?
            .collect();
        // Docker publishes on IPv4 and `localhost` lists ::1 first; on Windows a refused loopback
        // connection takes about two seconds to fail, so try IPv4 first and then the address that
        // worked last time, which comes first of all.
        addresses.sort_by_key(|a| !a.is_ipv4());
        if let Some(good) = self.good.lock().expect("address cache").get(&key).copied() {
            addresses.retain(|a| *a != good);
            addresses.insert(0, good);
        }
        let mut last = format!("no address for {}", target.connect_host);
        for address in addresses {
            match TcpStream::connect_timeout(&address, self.timeout) {
                Ok(stream) => {
                    stream
                        .set_read_timeout(Some(self.timeout))
                        .map_err(|e| e.to_string())?;
                    stream
                        .set_write_timeout(Some(self.timeout))
                        .map_err(|e| e.to_string())?;
                    self.good
                        .lock()
                        .expect("address cache")
                        .insert(key, address);
                    return Ok(stream);
                }
                Err(e) => {
                    last = format!(
                        "cannot connect to {}:{}: {e}",
                        target.connect_host, target.port
                    )
                }
            }
        }
        Err(last)
    }
}

fn request_text(path: &str, host_header: &str) -> String {
    format!(
        "GET {path} HTTP/1.0\r\nHost: {host_header}\r\nUser-Agent: dynodoctl/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        crate::VERSION
    )
}

/// A socket with an overall deadline. The per-operation timeouts alone let a server that sends a
/// byte every few seconds keep a run waiting for hours; here every read and write is armed with
/// what is left of the whole request's time (TLS handshake included).
struct Limited {
    tcp: TcpStream,
    deadline: Instant,
    per_operation: Duration,
}

impl Limited {
    fn new(tcp: TcpStream, overall: Duration, per_operation: Duration) -> Self {
        Self {
            tcp,
            deadline: Instant::now() + overall,
            per_operation,
        }
    }

    fn arm(&self) -> std::io::Result<()> {
        let left = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the request took longer than the overall time limit",
                )
            })?;
        let each = left.min(self.per_operation);
        self.tcp.set_read_timeout(Some(each))?;
        self.tcp.set_write_timeout(Some(each))
    }
}

impl Read for Limited {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.arm()?;
        self.tcp.read(buf)
    }
}

impl Write for Limited {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.arm()?;
        self.tcp.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.tcp.flush()
    }
}

/// Read until the server closes (HTTP/1.0 with Connection: close), tolerating an abrupt TLS close.
/// A timeout, or an answer larger than the limit, is an error, never a silently short answer.
fn read_all(stream: &mut dyn Read) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                data.extend_from_slice(&buffer[..n]);
                if data.len() > MAX_BODY + 64 * 1024 {
                    return Err(format!(
                        "the answer is larger than the {} MiB this tool reads",
                        MAX_BODY >> 20
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !data.is_empty() => break,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(format!("read timed out ({e})"));
            }
            Err(e) => {
                if data.is_empty() {
                    return Err(format!("read failed: {e}"));
                }
                break;
            }
        }
    }
    Ok(data)
}

/// Parse a complete HTTP/1.x response (status line, headers, body; chunked bodies decoded).
pub fn parse_response(data: &[u8]) -> Result<HttpResponse, String> {
    let split = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the answer is not HTTP (no header end)")?;
    let head = String::from_utf8_lossy(&data[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.") {
        return Err(format!("the answer is not HTTP ({status_line:.40})"));
    }
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or("the status line has no status code")?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            l.split_once(':')
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    let raw_body = &data[split + 4..];
    let chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"));
    let body = if chunked {
        decode_chunked(raw_body)?
    } else {
        raw_body.to_vec()
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("damaged chunked body")?;
        let size_text = String::from_utf8_lossy(&data[..eol]).into_owned();
        let digits = size_text.split(';').next().unwrap_or("").trim();
        // A hostile server can write any number here: parse it with a length limit, then compare
        // before adding (a size near usize::MAX made `size + 2` overflow and panic).
        if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("damaged chunk size".into());
        }
        let size = usize::from_str_radix(digits, 16).map_err(|_| "damaged chunk size")?;
        if size > MAX_BODY || out.len() + size > MAX_BODY {
            return Err(format!(
                "the chunked body is larger than the {} MiB this tool reads",
                MAX_BODY >> 20
            ));
        }
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size + 2 {
            return Err("truncated chunked body".into());
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

#[derive(Debug)]
struct Recording {
    inner: Arc<WebPkiServerVerifier>,
    provider: Arc<rustls::crypto::CryptoProvider>,
    outcome: Mutex<Option<Result<(), String>>>,
}

impl ServerCertVerifier for Recording {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let result = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        *self.outcome.lock().expect("verifier lock") =
            Some(result.as_ref().map(|_| ()).map_err(describe_tls_error));
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Plain words for the certificate problems an operator meets.
pub fn describe_tls_error(error: &TlsError) -> String {
    let text = error.to_string();
    if text.contains("UnknownIssuer") {
        "the certificate was not issued by an authority this tool trusts (self-signed, or a company authority: pass it with --ca-file)".into()
    } else if text.contains("NotValidForName") || text.contains("NotValidForNameContext") {
        "the certificate does not list the name in ORIGIN".into()
    } else if text.contains("Expired") {
        "the certificate has expired".into()
    } else if text.contains("NotValidYet") {
        "the certificate is not valid yet (check this machine's clock)".into()
    } else {
        format!("the certificate was refused: {text}")
    }
}

impl Net for RealNet {
    fn https_get(&self, target: &Target, path: &str) -> Result<(HttpResponse, TlsFacts), String> {
        let shared = self.shared()?;
        let recording = Arc::new(Recording {
            inner: shared.verifier.clone(),
            provider: shared.provider.clone(),
            outcome: Mutex::new(None),
        });
        let config = rustls::ClientConfig::builder_with_provider(shared.provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(recording.clone())
            .with_no_client_auth();
        let name = ServerName::try_from(target.server_name.clone())
            .map_err(|_| format!("`{}` is not a valid server name", target.server_name))?;
        let mut connection =
            rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
        let mut tcp = Limited::new(self.connect(target)?, self.deadline, self.timeout);
        let mut stream = rustls::Stream::new(&mut connection, &mut tcp);
        stream
            .write_all(request_text(path, &target.host_header).as_bytes())
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
        let data = read_all(&mut stream).map_err(|e| format!("TLS read failed: {e}"))?;
        let facts = TlsFacts {
            protocol: connection
                .protocol_version()
                .map(|v| format!("{v:?}"))
                .unwrap_or_default(),
            cipher: connection
                .negotiated_cipher_suite()
                .map(|s| format!("{:?}", s.suite()))
                .unwrap_or_default(),
            chain: connection
                .peer_certificates()
                .map(|c| c.iter().map(|d| d.as_ref().to_vec()).collect())
                .unwrap_or_default(),
            trust: recording
                .outcome
                .lock()
                .expect("verifier lock")
                .clone()
                .unwrap_or_else(|| Err("no certificate was presented".into())),
        };
        Ok((parse_response(&data)?, facts))
    }

    fn http_get(&self, target: &Target, path: &str) -> Result<HttpResponse, String> {
        let mut tcp = Limited::new(self.connect(target)?, self.deadline, self.timeout);
        tcp.write_all(request_text(path, &target.host_header).as_bytes())
            .map_err(|e| format!("write failed: {e}"))?;
        parse_response(&read_all(&mut tcp)?)
    }
}

/// What a TLS request returns: the response and the handshake facts, or why it failed.
pub type HttpsAnswer = Result<(HttpResponse, TlsFacts), String>;

/// A scripted [`Net`] for tests.
#[derive(Default)]
pub struct FakeNet {
    pub https: std::cell::RefCell<Vec<(String, HttpsAnswer)>>,
    pub http: std::cell::RefCell<Vec<(String, Result<HttpResponse, String>)>>,
    pub requested: std::cell::RefCell<Vec<String>>,
}

impl FakeNet {
    /// A later registration for the same path replaces the earlier one.
    pub fn on_https(&self, path: &str, answer: Result<(HttpResponse, TlsFacts), String>) {
        self.https
            .borrow_mut()
            .insert(0, (path.to_string(), answer));
    }
    pub fn on_http(&self, path: &str, answer: Result<HttpResponse, String>) {
        self.http.borrow_mut().insert(0, (path.to_string(), answer));
    }
}

/// A path registered as `@host/path` answers only requests whose Host header is `host`.
impl Net for FakeNet {
    fn https_get(&self, target: &Target, path: &str) -> Result<(HttpResponse, TlsFacts), String> {
        self.requested.borrow_mut().push(format!(
            "https://{}{path} via {}:{}",
            target.host_header, target.connect_host, target.port
        ));
        let specific = format!("@{}{path}", target.host_header);
        let answers = self.https.borrow();
        answers
            .iter()
            .find(|(p, _)| *p == specific)
            .or_else(|| answers.iter().find(|(p, _)| p == path))
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| Err(format!("no scripted answer for https {path}")))
    }
    fn http_get(&self, target: &Target, path: &str) -> Result<HttpResponse, String> {
        self.requested.borrow_mut().push(format!(
            "http://{}{path} via {}:{}",
            target.host_header, target.connect_host, target.port
        ));
        self.http
            .borrow()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| Err(format!("no scripted answer for http {path}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_headers_and_body() {
        let raw = b"HTTP/1.1 301 Moved Permanently\r\nLocation: https://docs.example.org/x\r\nContent-Security-Policy: a\r\nContent-Security-Policy: b\r\n\r\nbody";
        let r = parse_response(raw).unwrap();
        assert_eq!(r.status, 301);
        assert_eq!(r.header("location"), Some("https://docs.example.org/x"));
        assert_eq!(r.header_count("content-security-policy"), 2);
        assert_eq!(r.text(), "body");
    }

    #[test]
    fn decodes_chunked_bodies() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw).unwrap().text(), "hello world");
    }

    #[test]
    fn rejects_answers_that_are_not_http() {
        assert!(parse_response(b"SSH-2.0-OpenSSH\r\n\r\n").is_err());
        assert!(parse_response(b"garbage").is_err());
        assert!(parse_response(b"HTTP/1.1 abc\r\n\r\n").is_err());
        assert!(
            parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n").is_err()
        );
    }

    #[test]
    fn a_hostile_chunk_size_is_an_error_not_an_overflow_panic() {
        // M4: `size + 2` overflowed for a size near usize::MAX (a panic in a debug build).
        for size in [
            "ffffffffffffffff",
            "7fffffffffffffff",
            "fffffffe",
            "ffffffff",
            "100000000",
            "00000000000000005",
            "-1",
            "",
            "zz",
        ] {
            let raw = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{size}\r\nabc\r\n0\r\n\r\n"
            );
            let result = std::panic::catch_unwind(|| parse_response(raw.as_bytes()));
            assert!(result.is_ok(), "panicked on chunk size {size:?}");
            assert!(result.unwrap().is_err(), "accepted chunk size {size:?}");
        }
        // Many small chunks that together pass the limit are refused too.
        let mut body = String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
        let chunk = "x".repeat(1 << 20);
        for _ in 0..7 {
            body.push_str(&format!("100000\r\n{chunk}\r\n"));
        }
        body.push_str("0\r\n\r\n");
        assert!(parse_response(body.as_bytes())
            .unwrap_err()
            .contains("larger than"));
    }

    #[test]
    fn a_server_that_drips_bytes_is_cut_off_by_the_overall_deadline() {
        // M4: per-read timeouts alone never fire when a byte arrives every few seconds.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut seen = [0u8; 1024];
            let _ = socket.read(&mut seen);
            let _ = socket.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
            while !flag.load(std::sync::atomic::Ordering::SeqCst) {
                if socket.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let mut net = RealNet::new(vec![]);
        net.timeout = Duration::from_secs(5);
        net.deadline = Duration::from_millis(800);
        let target = Target {
            connect_host: "127.0.0.1".into(),
            port,
            server_name: "docs.example.org".into(),
            host_header: "docs.example.org".into(),
        };
        let started = Instant::now();
        let e = net.http_get(&target, "/").unwrap_err();
        let took = started.elapsed();
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        server.join().unwrap();
        assert!(e.contains("timed out"), "{e}");
        assert!(took < Duration::from_secs(4), "{took:?}");
    }

    #[test]
    fn an_answer_larger_than_the_limit_is_an_error_not_a_silent_truncation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut seen = [0u8; 1024];
            let _ = socket.read(&mut seen);
            let _ = socket.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
            let block = vec![b'x'; 1 << 20];
            for _ in 0..8 {
                if socket.write_all(&block).is_err() {
                    break;
                }
            }
        });
        let net = RealNet::new(vec![]);
        let target = Target {
            connect_host: "127.0.0.1".into(),
            port,
            server_name: "docs.example.org".into(),
            host_header: "docs.example.org".into(),
        };
        let e = net.http_get(&target, "/").unwrap_err();
        let _ = server.join();
        assert!(e.contains("larger than"), "{e}");
    }

    #[test]
    fn tls_errors_are_explained_in_plain_words() {
        let unknown = TlsError::InvalidCertificate(rustls::CertificateError::UnknownIssuer);
        assert!(describe_tls_error(&unknown).contains("--ca-file"));
        let name = TlsError::InvalidCertificate(rustls::CertificateError::NotValidForName);
        assert!(describe_tls_error(&name).contains("ORIGIN"));
        let expired = TlsError::InvalidCertificate(rustls::CertificateError::Expired);
        assert!(describe_tls_error(&expired).contains("expired"));
    }

    #[test]
    fn a_closed_port_is_an_error_not_a_hang() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut net = RealNet::new(vec![]);
        net.timeout = Duration::from_secs(2);
        let target = Target {
            connect_host: "127.0.0.1".into(),
            port,
            server_name: "localhost".into(),
            host_header: "localhost".into(),
        };
        assert!(net.http_get(&target, "/").is_err());
        assert!(net.https_get(&target, "/").is_err());
    }

    #[test]
    fn the_address_that_worked_is_remembered_and_ipv4_is_tried_first() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let net = RealNet::new(vec![]);
        let target = Target {
            connect_host: "localhost".into(),
            port,
            server_name: "localhost".into(),
            host_header: "localhost".into(),
        };
        let started = std::time::Instant::now();
        net.connect(&target).expect("connects to localhost");
        let remembered = *net
            .good
            .lock()
            .unwrap()
            .get(&format!("localhost:{port}"))
            .unwrap();
        assert!(remembered.is_ipv4(), "{remembered}");
        net.connect(&target).expect("second connection");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "no per-request wait for a refused ::1"
        );
    }

    #[test]
    fn plain_http_round_trip_over_a_loopback_socket() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut seen = [0u8; 1024];
            let n = socket.read(&mut seen).unwrap();
            let request = String::from_utf8_lossy(&seen[..n]).into_owned();
            socket
                .write_all(b"HTTP/1.0 200 OK\r\nX-Test: yes\r\n\r\nhi")
                .unwrap();
            request
        });
        let net = RealNet::new(vec![]);
        let target = Target {
            connect_host: "127.0.0.1".into(),
            port,
            server_name: "docs.example.org".into(),
            host_header: "docs.example.org".into(),
        };
        let r = net.http_get(&target, "/healthz").unwrap();
        assert_eq!((r.status, r.text()), (200, "hi".to_string()));
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /healthz HTTP/1.0\r\nHost: docs.example.org\r\n"));
    }
}
