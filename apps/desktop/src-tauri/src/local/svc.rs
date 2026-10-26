//! The local counterpart of the hosted pre-alpha service's `/svc/v1` routes: what the client keeps
//! and the engine has no place for. Same paths, same answers, so `data/remote.ts` is the same code
//! for the local workspace and for a server. There is one person here, so there is no account
//! check beyond the app's own.

use super::{appdb, Local};
use crate::remote::{Method, Reply};
use serde_json::{json, Value};

fn reply(status: u16, body: Value) -> Reply {
    Reply { status, body }
}

/// The document ids in `notes?document=a&document=b`.
pub fn documents_in(query: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|pair| pair.strip_prefix("document="))
        .map(str::to_string)
        .collect()
}

impl Local {
    /// One request of the client to its own state. `path_and_query` passed `remote::allowed`.
    pub async fn svc(&self, method: Method, path_and_query: &str, body: Option<&Value>) -> Reply {
        let (path, query) = path_and_query
            .split_once('?')
            .unwrap_or((path_and_query, ""));
        let Ok(app) = self.app().await else {
            return reply(503, json!({ "error": "The workspace is not ready." }));
        };
        let me = self.identity().await.unwrap_or_else(|| "local".to_string());
        match (method, path) {
            (Method::Get, "session") => reply(
                200,
                json!({ "identity_id": me, "visitor": null, "local": true }),
            ),
            (Method::Get, p) if p.starts_with("state/") => {
                match appdb::get_state(&app, &p[6..]).await {
                    Ok(v) => reply(200, v),
                    Err(e) => reply(404, json!({ "error": e.0 })),
                }
            }
            (Method::Put, p) if p.starts_with("state/") => {
                let Some(value) = body.and_then(|b| b.get("value")) else {
                    return reply(400, json!({ "error": "Send {\"value\": ...}" }));
                };
                match appdb::put_state(&app, &p[6..], value).await {
                    Ok(v) => reply(200, v),
                    Err(e) if e.0 == "Too large." => reply(413, json!({ "error": e.0 })),
                    Err(e) => reply(400, json!({ "error": e.0 })),
                }
            }
            (Method::Get, "notes") => match appdb::notes_for(&app, &documents_in(query)).await {
                Ok(v) => reply(200, v),
                Err(e) => reply(500, json!({ "error": e.0 })),
            },
            (Method::Post, "notes") => match body {
                Some(n) => match appdb::add_note(&app, n, &me).await {
                    Ok(v) => reply(200, v),
                    Err(e) if e.0 == "Invalid note" => reply(400, json!({ "error": e.0 })),
                    Err(e) => reply(429, json!({ "error": e.0 })),
                },
                None => reply(400, json!({ "error": "Invalid note" })),
            },
            _ => reply(404, json!({ "error": "Not found" })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_are_read_from_the_query() {
        assert_eq!(
            documents_in("document=a&document=b&limit=3"),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(documents_in("").is_empty());
    }
}
