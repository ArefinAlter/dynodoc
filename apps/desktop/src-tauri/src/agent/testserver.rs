//! A small HTTP server on loopback for the tests: it records each request and answers from a
//! script, in order (the last answer repeats). Streams are sent as chunked server-sent events.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub enum Reply {
    /// A whole answer: status, extra headers, body.
    Plain(u16, Vec<(&'static str, &'static str)>, String),
    /// A stream: each chunk is sent, then the pause.
    Sse(Vec<String>, Duration),
    /// Accepts and says nothing for a long time.
    Silent,
}

pub struct Server {
    pub addr: SocketAddr,
    pub seen: Arc<Mutex<Vec<Recorded>>>,
}

impl Server {
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.seen.lock().unwrap().clone()
    }
}

pub async fn start(script: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Arc<Mutex<Vec<Recorded>>> = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        let mut n = 0usize;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let reply = script.get(n).or(script.last()).cloned().unwrap_or(Reply::Plain(500, vec![], String::new()));
            n += 1;
            let log = log.clone();
            tokio::spawn(async move {
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 4096];
                let head_end = loop {
                    let Ok(k) = sock.read(&mut tmp).await else { return };
                    if k == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..k]);
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break p + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let mut lines = head.lines();
                let first = lines.next().unwrap_or("").to_string();
                let mut it = first.split_whitespace();
                let (method, path) = (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string());
                let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
                let len = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
                while buf.len() < head_end + len {
                    let Ok(k) = sock.read(&mut tmp).await else { return };
                    if k == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..k]);
                }
                let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                log.lock().unwrap().push(Recorded { method, path, headers, body });
                match reply {
                    Reply::Plain(status, extra, text) => {
                        let mut out = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n", text.len());
                        for (k, v) in extra {
                            out.push_str(&format!("{k}: {v}\r\n"));
                        }
                        out.push_str("\r\n");
                        out.push_str(&text);
                        let _ = sock.write_all(out.as_bytes()).await;
                    }
                    Reply::Sse(chunks, pause) => {
                        let _ = sock.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n").await;
                        for c in chunks {
                            let frame = format!("{:x}\r\n{}\r\n", c.len(), c);
                            if sock.write_all(frame.as_bytes()).await.is_err() {
                                return;
                            }
                            let _ = sock.flush().await;
                            tokio::time::sleep(pause).await;
                        }
                        let _ = sock.write_all(b"0\r\n\r\n").await;
                    }
                    Reply::Silent => {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
                let _ = sock.shutdown().await;
            });
        }
    });
    Server { addr, seen }
}
