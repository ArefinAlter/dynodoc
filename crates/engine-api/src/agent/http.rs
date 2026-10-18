//! HTTP details shared by the agent routes (protocol spec 12.2, 12.3): content
//! negotiation between the DCT body and the JSON mirror, ETags, and the request headers
//! that are recorded as claims.

use axum::{
    body::Body,
    http::{
        header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH, VARY},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::Response,
};
use engine_agent::{error::AgentError, rev::RevToken};

/// The media type of a DCT body.
pub const DCT_MEDIA: &str = "text/vnd.dynodoc.dct";

/// How a response body is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `text/vnd.dynodoc.dct; v=1`
    Dct,
    /// `application/json`, the mirror of spec 12.3. The default for plain HTTP clients.
    Json,
}

impl Format {
    fn content_type(self) -> &'static str {
        match self {
            Format::Dct => "text/vnd.dynodoc.dct; v=1; charset=utf-8",
            Format::Json => "application/json",
        }
    }
}

/// Pick the representation from `Accept`. DCT only when it is asked for by name
/// (`text/vnd.dynodoc.dct`, optionally `v=1`); a missing header, `*/*`, `application/*`
/// and `application/json` are JSON. A DCT version other than 1 is refused rather than
/// answered with something the client cannot parse.
pub fn negotiate(accept: Option<&str>) -> Result<Format, AgentError> {
    let Some(raw) = accept else {
        return Ok(Format::Json);
    };
    // (q in thousandths, specificity, format); the first best candidate wins ties.
    let mut best: Option<(u32, u8, Format)> = None;
    for range in raw.split(',') {
        let mut parts = range.split(';');
        let media = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let mut q = 1000u32;
        let mut version = None;
        for param in parts {
            let Some((name, value)) = param.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match name.trim().to_ascii_lowercase().as_str() {
                "q" => {
                    q = value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| (0.0..=1.0).contains(v))
                        .map_or(0, |v| (v * 1000.0).round() as u32)
                }
                "v" => version = Some(value.to_owned()),
                _ => {}
            }
        }
        let candidate = match media.as_str() {
            DCT_MEDIA => {
                if version.as_deref().is_some_and(|v| v != "1") {
                    return Err(AgentError::BadRequest { field: "accept" });
                }
                Some((2, Format::Dct))
            }
            "application/json" => Some((2, Format::Json)),
            "application/*" => Some((1, Format::Json)),
            "*/*" => Some((0, Format::Json)),
            _ => None,
        };
        if let Some((specificity, format)) = candidate.filter(|_| q > 0) {
            if best.is_none_or(|(bq, bs, _)| (q, specificity) > (bq, bs)) {
                best = Some((q, specificity, format));
            }
        }
    }
    Ok(best.map_or(Format::Json, |(_, _, f)| f))
}

/// Does `If-None-Match` name `token`? Accepts `*`, a list, and weak validators.
pub fn if_none_match(headers: &HeaderMap, token: &RevToken) -> bool {
    let Some(value) = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let ours = token.etag();
    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == ours
    })
}

/// The `Accept` header as text, if it is valid text.
pub fn accept(headers: &HeaderMap) -> Option<&str> {
    headers.get(ACCEPT).and_then(|v| v.to_str().ok())
}

/// A successful body. `token` adds the ETag of the revision it was computed at.
pub fn reply(format: Format, body: String, token: Option<&RevToken>) -> Response {
    let mut response = Response::new(Body::from(body));
    stamp(response.headers_mut(), Some(format), token);
    response
}

/// `304 Not Modified` for a revision the caller already holds.
pub fn not_modified(token: &RevToken) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    stamp(response.headers_mut(), None, Some(token));
    response
}

fn stamp(headers: &mut HeaderMap, format: Option<Format>, token: Option<&RevToken>) {
    if let Some(format) = format {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(format.content_type()));
    }
    // Document content: a cache may keep it only for this user, and must revalidate.
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    headers.insert(VARY, HeaderValue::from_static("Accept"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Some(token) = token {
        if let Ok(value) = HeaderValue::from_str(&token.etag()) {
            headers.insert(ETAG, value);
        }
    }
}

/// `X-Dynodoc-Client`: the caller's self-description, kept as a claim (at most 120
/// printable ASCII characters). It is never used for authorisation.
pub fn client_claim(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("x-dynodoc-client")?.to_str().ok()?;
    let clean: String = value
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(120)
        .collect();
    let clean = clean.trim().to_owned();
    (!clean.is_empty()).then_some(clean)
}

/// `Traceparent` in the W3C form `00-<32 hex>-<16 hex>-<2 hex>` (lowercase), else `None`.
pub fn traceparent(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("traceparent")?.to_str().ok()?.trim();
    let parts: Vec<&str> = value.split('-').collect();
    let ok = parts.len() == 4
        && [2, 32, 16, 2]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    ok.then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_agent::rev::Chain8;

    fn pick(accept: &str) -> Result<Format, AgentError> {
        negotiate(Some(accept))
    }

    #[test]
    fn json_is_the_default_and_dct_must_be_asked_for() {
        assert_eq!(negotiate(None).unwrap(), Format::Json);
        for a in ["*/*", "application/json", "application/*", "text/html", ""] {
            assert_eq!(pick(a).unwrap(), Format::Json, "{a}");
        }
        for a in [
            "text/vnd.dynodoc.dct",
            "text/vnd.dynodoc.dct; v=1",
            "TEXT/VND.DYNODOC.DCT;v=\"1\"",
            "text/vnd.dynodoc.dct; v=1, application/json;q=0.5",
            "application/json;q=0.2, text/vnd.dynodoc.dct",
            "*/*;q=0.1, text/vnd.dynodoc.dct",
        ] {
            assert_eq!(pick(a).unwrap(), Format::Dct, "{a}");
        }
        // Equal preference: the first listed wins.
        assert_eq!(
            pick("application/json, text/vnd.dynodoc.dct").unwrap(),
            Format::Json
        );
        // q=0 excludes a type.
        assert_eq!(
            pick("text/vnd.dynodoc.dct;q=0, application/json").unwrap(),
            Format::Json
        );
        assert!(matches!(
            pick("text/vnd.dynodoc.dct; v=2"),
            Err(AgentError::BadRequest { field: "accept" })
        ));
    }

    #[test]
    fn if_none_match_accepts_lists_weak_tags_and_star() {
        let token = RevToken::new(412, Chain8::from_hex("9f3a1c04ffff").unwrap());
        let with = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(IF_NONE_MATCH, HeaderValue::from_str(v).unwrap());
            h
        };
        assert!(if_none_match(&with("\"412:9f3a1c04\""), &token));
        assert!(if_none_match(&with("W/\"412:9f3a1c04\""), &token));
        assert!(if_none_match(&with("\"1:aaaaaaaa\", \"412:9f3a1c04\""), &token));
        assert!(if_none_match(&with("*"), &token));
        assert!(!if_none_match(&with("\"412:9f3a1c05\""), &token));
        assert!(!if_none_match(&with("\"411:9f3a1c04\""), &token));
        assert!(!if_none_match(&HeaderMap::new(), &token));
    }

    #[test]
    fn claims_are_sanitised_and_traceparent_is_strict() {
        let mut h = HeaderMap::new();
        h.insert("x-dynodoc-client", HeaderValue::from_static("claude-code/2.1 (x)"));
        assert_eq!(client_claim(&h).unwrap(), "claude-code/2.1 (x)");
        h.insert("x-dynodoc-client", HeaderValue::from_static("   "));
        assert_eq!(client_claim(&h), None);
        h.insert(
            "x-dynodoc-client",
            HeaderValue::from_str(&"a".repeat(500)).unwrap(),
        );
        assert_eq!(client_claim(&h).unwrap().len(), 120);
        let good = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        h.insert("traceparent", HeaderValue::from_static(good));
        assert_eq!(traceparent(&h).as_deref(), Some(good));
        for bad in [
            "00-0AF7651916CD43DD8448EB211C80319C-b7ad6b7169203331-01",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331",
            "garbage",
        ] {
            h.insert("traceparent", HeaderValue::from_str(bad).unwrap());
            assert_eq!(traceparent(&h), None, "{bad}");
        }
    }
}
