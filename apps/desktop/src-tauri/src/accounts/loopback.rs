//! The app's own listener for the browser sign-in (decision 020).
//!
//! The server's confirmation page, after the person pressed Continue, sends the browser to
//! `http://127.0.0.1:<port>/signin?code=..&state=..`, where `<port>` is the one this app named when
//! it opened the page (RFC 8252 section 7.3). The listener here is bound to 127.0.0.1 only, serves
//! the one sign-in that is waiting, and answers nothing else:
//!
//! - one fixed path and method, `GET /signin`, with exactly the two values, each once, and in the
//!   shapes the server makes (a code of 64 lower-case hexadecimal digits, a state of 16 to 64 URL-safe
//!   characters); anything else is refused without reading further;
//! - the `Host` header must name this listener (127.0.0.1 and its port), which stops a web page that
//!   has been made to resolve its own name to this address from talking to it (DNS rebinding);
//! - a request a browser marks as anything but a top-level navigation (an image, a script, a
//!   `fetch`) is refused, so a page cannot trigger the listener behind the person's back;
//! - the state must be the waiting attempt's (`signin::Pending::complete`, compared without
//!   stopping at the first difference); a request with another state changes nothing;
//! - the answer to a good request is a redirect to a fixed page that says nothing about the code,
//!   so the code does not stay in the browser's history.
//!
//! A code in an address is not a credential on its own: it works once, for 60 seconds, and only with
//! the verifier that never leaves this app (PKCE, RFC 7636).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Most bytes of a request this listener reads.
pub const MAX_HEAD: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// A well-formed hand-over: the code and the state.
    SignIn { code: String, state: String },
    /// The page the browser is sent to afterwards.
    Done,
    /// Refused, with the status to answer.
    Refuse(u16),
}

fn hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn state_ok(s: &str) -> bool {
    (16..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Reads the request head the browser sent and says what it is. `port` is this listener's.
pub fn parse(head: &str, port: u16) -> Request {
    let mut lines = head.split("\r\n");
    let Some(line) = lines.next() else {
        return Request::Refuse(400);
    };
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Request::Refuse(400);
    };
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Request::Refuse(400);
    }
    let want_host = format!("127.0.0.1:{port}");
    let mut host_seen = false;
    for header in lines {
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return Request::Refuse(400);
        };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        match name.as_str() {
            "host" => {
                if host_seen || value != want_host {
                    return Request::Refuse(400);
                }
                host_seen = true;
            }
            "sec-fetch-mode" if value != "navigate" => return Request::Refuse(403),
            "sec-fetch-dest" if value != "document" => return Request::Refuse(403),
            _ => {}
        }
    }
    if !host_seen {
        return Request::Refuse(400);
    }
    if method != "GET" {
        return Request::Refuse(405);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match path {
        "/done" if query.is_empty() => Request::Done,
        "/signin" => {
            let pairs: Vec<(&str, &str)> =
                query.split('&').filter_map(|p| p.split_once('=')).collect();
            if pairs.len() != 2 || query.split('&').count() != 2 {
                return Request::Refuse(400);
            }
            let find = |name: &str| {
                let mut found = pairs.iter().filter(|(n, _)| *n == name).map(|(_, v)| *v);
                let first = found.next()?;
                found.next().is_none().then_some(first)
            };
            match (find("code"), find("state")) {
                (Some(code), Some(state)) if hex64(code) && state_ok(state) => Request::SignIn {
                    code: code.to_string(),
                    state: state.to_string(),
                },
                _ => Request::Refuse(400),
            }
        }
        _ => Request::Refuse(404),
    }
}

/// The request head from the stream: up to the blank line, at most [`MAX_HEAD`] bytes, in 5 seconds.
pub async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let work = async {
        let mut buffer = Vec::with_capacity(512);
        let mut chunk = [0u8; 512];
        loop {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..n]);
            if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                buffer.truncate(end + 2);
                return String::from_utf8(buffer).ok();
            }
            if buffer.len() > MAX_HEAD {
                return None;
            }
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), work)
        .await
        .ok()
        .flatten()
}

const DONE_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Dynodoc</title>\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem;color:#1b1b1f}</style></head>\
<body><h1>Back to the Dynodoc app</h1>\
<p>The sign-in was handed to the app on this computer. Go back to it: it tells you whether it worked.</p>\
<p>You can close this tab.</p></body></html>";

fn response(status: u16, extra: &str, content_type: &str, body: &str) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// What to write for a request.
pub fn answer(request: &Request, accepted: bool) -> Vec<u8> {
    match request {
        Request::SignIn { .. } if accepted => {
            response(303, "Location: /done\r\n", "text/plain; charset=utf-8", "")
        }
        Request::SignIn { .. } => response(
            400,
            "",
            "text/plain; charset=utf-8",
            "This sign-in was not started by this app.",
        ),
        Request::Done => response(200, "", "text/html; charset=utf-8", DONE_PAGE),
        Request::Refuse(status) => {
            response(*status, "", "text/plain; charset=utf-8", "Not available.")
        }
    }
}

pub async fn write(stream: &mut TcpStream, bytes: &[u8]) {
    let _ = stream.write_all(bytes).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const STATE: &str = "b3JkZXJseS1zdGF0ZS12YWx1ZQ";

    fn get(target: &str, port: u16, extra: &str) -> String {
        format!(
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUser-Agent: test\r\n{extra}\r\n"
        )
    }

    #[test]
    fn the_one_good_request_is_read() {
        let head = get(
            &format!("/signin?code={CODE}&state={STATE}"),
            49321,
            "Sec-Fetch-Mode: navigate\r\nSec-Fetch-Dest: document\r\n",
        );
        assert_eq!(
            parse(&head, 49321),
            Request::SignIn {
                code: CODE.into(),
                state: STATE.into()
            }
        );
        let reversed = get(&format!("/signin?state={STATE}&code={CODE}"), 49321, "");
        assert_eq!(
            parse(&reversed, 49321),
            Request::SignIn {
                code: CODE.into(),
                state: STATE.into()
            }
        );
        assert_eq!(parse(&get("/done", 49321, ""), 49321), Request::Done);
    }

    #[test]
    fn everything_else_is_refused() {
        let good = format!("/signin?code={CODE}&state={STATE}");
        let cases: Vec<(String, u16)> = vec![
            // Not this listener's name or port (DNS rebinding).
            (get(&good, 49322, ""), 400),
            (
                format!("GET {good} HTTP/1.1\r\nHost: localhost:49321\r\n\r\n"),
                400,
            ),
            (
                format!("GET {good} HTTP/1.1\r\nHost: evil.example\r\n\r\n"),
                400,
            ),
            (format!("GET {good} HTTP/1.1\r\n\r\n"), 400),
            (
                format!(
                    "GET {good} HTTP/1.1\r\nHost: 127.0.0.1:49321\r\nHost: 127.0.0.1:49321\r\n\r\n"
                ),
                400,
            ),
            // Not a navigation.
            (
                get(
                    &good,
                    49321,
                    "Sec-Fetch-Mode: no-cors\r\nSec-Fetch-Dest: image\r\n",
                ),
                403,
            ),
            (get(&good, 49321, "Sec-Fetch-Mode: cors\r\n"), 403),
            (get(&good, 49321, "Sec-Fetch-Dest: script\r\n"), 403),
            // The wrong method, path, shape or count of values.
            (
                format!("POST {good} HTTP/1.1\r\nHost: 127.0.0.1:49321\r\n\r\n"),
                405,
            ),
            (get("/", 49321, ""), 404),
            (get("/signin/", 49321, ""), 404),
            (get("/admin", 49321, ""), 404),
            (get("/done?x=1", 49321, ""), 404),
            (get("/signin", 49321, ""), 400),
            (get(&format!("/signin?code={CODE}"), 49321, ""), 400),
            (
                get(
                    &format!("/signin?code={CODE}&state={STATE}&next=x"),
                    49321,
                    "",
                ),
                400,
            ),
            (
                get(&format!("/signin?code={CODE}&code={CODE}"), 49321, ""),
                400,
            ),
            (
                get(
                    &format!("/signin?code={}&state={STATE}", CODE.to_uppercase()),
                    49321,
                    "",
                ),
                400,
            ),
            (
                get(
                    &format!("/signin?code={}&state={STATE}", &CODE[1..]),
                    49321,
                    "",
                ),
                400,
            ),
            (
                get(&format!("/signin?code={CODE}&state=short"), 49321, ""),
                400,
            ),
            (
                get(&format!("/signin?code={CODE}&state={STATE}%0a"), 49321, ""),
                400,
            ),
            (
                format!("GET {good} HTTP/2\r\nHost: 127.0.0.1:49321\r\n\r\n"),
                400,
            ),
            ("nonsense".to_string(), 400),
        ];
        for (head, status) in cases {
            assert_eq!(parse(&head, 49321), Request::Refuse(status), "{head:?}");
        }
    }

    #[test]
    fn the_answers_never_repeat_the_code_and_the_good_one_redirects_away_from_it() {
        let ok = answer(
            &Request::SignIn {
                code: CODE.into(),
                state: STATE.into(),
            },
            true,
        );
        let text = String::from_utf8(ok).unwrap();
        assert!(text.starts_with("HTTP/1.1 303 See Other"));
        assert!(text.contains("Location: /done\r\n"));
        assert!(!text.contains(CODE) && !text.contains(STATE));
        let no = String::from_utf8(answer(
            &Request::SignIn {
                code: CODE.into(),
                state: STATE.into(),
            },
            false,
        ))
        .unwrap();
        assert!(no.starts_with("HTTP/1.1 400") && !no.contains(CODE));
        let done = String::from_utf8(answer(&Request::Done, true)).unwrap();
        assert!(
            done.contains("Content-Security-Policy: default-src 'none'")
                && done.contains("close this tab")
        );
        assert!(done.contains("Referrer-Policy: no-referrer"));
    }
}
