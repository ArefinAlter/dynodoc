//! JSON-RPC 2.0 framing for the stdio transport: one UTF-8 message per line.
//!
//! Decoding never panics and never trusts the shape of a message; every line that is not a
//! valid message is answered with a JSON-RPC error (or ignored when no answer is possible),
//! so stdout carries nothing but protocol messages whatever stdin sends.

use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// One decoded client message.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// An answer to a request this server never sent. Ignored.
    Response,
    /// Not a valid message. `id` is the request id when one could be read, else null.
    Invalid {
        id: Value,
        code: i64,
        message: &'static str,
    },
}

fn invalid(id: Value, code: i64, message: &'static str) -> Incoming {
    Incoming::Invalid { id, code, message }
}

/// Decode one line.
pub fn parse(line: &str) -> Incoming {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return invalid(Value::Null, PARSE_ERROR, "Parse error");
    };
    let Value::Object(mut message) = value else {
        // Includes batch arrays: the 2025-06-18 revision removed JSON-RPC batching.
        return invalid(Value::Null, INVALID_REQUEST, "Invalid Request");
    };
    // MCP forbids a null id; a float is not an id either.
    let id = match message.remove("id") {
        None => None,
        Some(id @ Value::String(_)) => Some(id),
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => Some(Value::Number(n)),
        Some(_) => return invalid(Value::Null, INVALID_REQUEST, "Invalid Request"),
    };
    let id_or_null = || id.clone().unwrap_or(Value::Null);
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid(id_or_null(), INVALID_REQUEST, "Invalid Request");
    }
    match message.remove("method") {
        Some(Value::String(method)) => {
            let params = message.remove("params").unwrap_or(Value::Null);
            match id {
                Some(id) => Incoming::Request { id, method, params },
                None => Incoming::Notification { method, params },
            }
        }
        Some(_) => invalid(id_or_null(), INVALID_REQUEST, "Invalid Request"),
        None if id.is_some()
            && (message.contains_key("result") || message.contains_key("error")) =>
        {
            Incoming::Response
        }
        None => invalid(id_or_null(), INVALID_REQUEST, "Invalid Request"),
    }
}

/// A successful response, as one line without the trailing newline.
pub fn success(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

/// An error response, as one line without the trailing newline.
pub fn failure(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// What [`read_line`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    Eof,
    Text(String),
    /// The line exceeded the cap; it was read to its end and discarded.
    TooLong,
    /// The line was not UTF-8.
    NotUtf8,
}

/// Read one line of at most `cap` bytes, without its line ending. A longer line is
/// consumed and reported as [`Line::TooLong`] without being held in memory.
pub async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> std::io::Result<Line> {
    let mut line: Vec<u8> = Vec::new();
    let mut too_long = false;
    let mut seen_any = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if !seen_any {
                return Ok(Line::Eof);
            }
            break;
        }
        seen_any = true;
        let end = available.iter().position(|b| *b == b'\n');
        let chunk = &available[..end.unwrap_or(available.len())];
        if !too_long {
            if line.len() + chunk.len() > cap {
                too_long = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let consumed = end.map_or(available.len(), |i| i + 1);
        reader.consume(consumed);
        if end.is_some() {
            break;
        }
    }
    if too_long {
        return Ok(Line::TooLong);
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(match String::from_utf8(line) {
        Ok(text) => Line::Text(text),
        Err(_) => Line::NotUtf8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_notifications_and_responses_are_told_apart() {
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#),
            Incoming::Request {
                id: json!(7),
                method: "ping".into(),
                params: Value::Null
            }
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":"a","method":"x","params":{"k":1}}"#),
            Incoming::Request {
                id: json!("a"),
                method: "x".into(),
                params: json!({"k":1})
            }
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Incoming::Notification {
                method: "notifications/initialized".into(),
                params: Value::Null
            }
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":3,"result":{}}"#),
            Incoming::Response
        );
    }

    #[test]
    fn invalid_messages_keep_the_id_when_there_is_one() {
        let code = |line: &str| match parse(line) {
            Incoming::Invalid { id, code, .. } => (id, code),
            other => panic!("{other:?}"),
        };
        assert_eq!(code("{"), (Value::Null, PARSE_ERROR));
        assert_eq!(code("[]"), (Value::Null, INVALID_REQUEST));
        assert_eq!(code("3"), (Value::Null, INVALID_REQUEST));
        assert_eq!(
            code(r#"{"id":1,"method":"x"}"#),
            (json!(1), INVALID_REQUEST)
        );
        assert_eq!(
            code(r#"{"jsonrpc":"1.0","id":1,"method":"x"}"#),
            (json!(1), INVALID_REQUEST)
        );
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":1,"method":5}"#),
            (json!(1), INVALID_REQUEST)
        );
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":null,"method":"x"}"#),
            (Value::Null, INVALID_REQUEST)
        );
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":1.5,"method":"x"}"#),
            (Value::Null, INVALID_REQUEST)
        );
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":9}"#),
            (json!(9), INVALID_REQUEST)
        );
    }

    #[test]
    fn responses_are_single_lines() {
        let line = success(&json!(1), json!({"text":"a\nb\u{2028}c"}));
        assert!(!line.contains('\n'));
        let line = failure(&Value::Null, PARSE_ERROR, "Parse error");
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["error"]["code"],
            PARSE_ERROR
        );
    }

    #[tokio::test]
    async fn lines_are_bounded_and_the_stream_recovers() {
        let input = b"one\r\n\nthis line is far too long\nlast".to_vec();
        let mut reader = tokio::io::BufReader::with_capacity(4, &input[..]);
        assert_eq!(
            read_line(&mut reader, 10).await.unwrap(),
            Line::Text("one".into())
        );
        assert_eq!(
            read_line(&mut reader, 10).await.unwrap(),
            Line::Text(String::new())
        );
        assert_eq!(read_line(&mut reader, 10).await.unwrap(), Line::TooLong);
        assert_eq!(
            read_line(&mut reader, 10).await.unwrap(),
            Line::Text("last".into())
        );
        assert_eq!(read_line(&mut reader, 10).await.unwrap(), Line::Eof);
        let mut reader = &b"\xff\xfe\nok\n"[..];
        assert_eq!(read_line(&mut reader, 10).await.unwrap(), Line::NotUtf8);
        assert_eq!(
            read_line(&mut reader, 10).await.unwrap(),
            Line::Text("ok".into())
        );
    }
}
