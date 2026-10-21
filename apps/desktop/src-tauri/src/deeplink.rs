//! `dynodoc://` links. The operating system hands the raw text of any such link to the app,
//! whoever wrote it (a web page, a mail, a chat), so it is treated as hostile input.
//!
//! Accepted forms, and nothing else:
//!
//! ```text
//! dynodoc://open/workspace
//! dynodoc://open/projects/<uuid>
//! dynodoc://open/documents/<uuid>
//! ```
//!
//! The result is a path on the CONFIGURED server (`/workspace`, `/projects/<uuid>`,
//! `/documents/<uuid>`). A link never names a server, a query, a fragment, a port or
//! credentials, and never carries a token, so there is nothing to leak or redirect.

use std::fmt;

pub const SCHEME: &str = "dynodoc";
/// Far longer than any accepted link (`dynodoc://open/documents/` plus 36 characters).
pub const MAX_LINK_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Workspace,
    Project(String),
    Document(String),
}

impl Target {
    /// The server-relative path, e.g. `/projects/0b0f...`.
    pub fn path(&self) -> String {
        match self {
            Self::Workspace => "/workspace".to_string(),
            Self::Project(id) => format!("/projects/{id}"),
            Self::Document(id) => format!("/documents/{id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    TooLong,
    NotOurs,
    Malformed,
    NotAllowed,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "The link is too long.",
            Self::NotOurs => "The link is not a Dynodoc link.",
            Self::Malformed => "The link is not written in a form Dynodoc accepts.",
            Self::NotAllowed => "Dynodoc does not open that kind of link.",
        })
    }
}

pub fn parse(raw: &str) -> Result<Target, LinkError> {
    if raw.len() > MAX_LINK_LEN {
        return Err(LinkError::TooLong);
    }
    let prefix = format!("{SCHEME}://");
    let rest = match raw.get(..prefix.len()) {
        Some(head) if head.eq_ignore_ascii_case(&prefix) => &raw[prefix.len()..],
        _ => return Err(LinkError::NotOurs),
    };
    // Letters, digits, '-' and '/' only. That leaves out every character the parsers of
    // URLs disagree about: '%' '\' '?' '#' '@' ':' '.' ';' spaces and controls.
    if !rest
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'/')
    {
        return Err(LinkError::Malformed);
    }
    // One trailing slash is how some systems hand the link over.
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        ["open", "workspace"] => Ok(Target::Workspace),
        ["open", "projects", id] => uuid(id).map(Target::Project),
        ["open", "documents", id] => uuid(id).map(Target::Document),
        _ if parts.iter().any(|p| p.is_empty()) => Err(LinkError::Malformed),
        _ => Err(LinkError::NotAllowed),
    }
}

/// 8-4-4-4-12 hexadecimal digits with hyphens. Returned in lower case, so one document
/// has one path.
fn uuid(text: &str) -> Result<String, LinkError> {
    let shape_ok = text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        });
    if shape_ok {
        Ok(text.to_ascii_lowercase())
    } else {
        Err(LinkError::Malformed)
    }
}

/// Links that arrive before the app has finished starting (a second launch while the first
/// is still setting up). They are kept, a few and short ones only, and handled once the app
/// is ready; anything else is dropped.
#[derive(Debug, Default)]
pub struct LinkQueue {
    links: Vec<String>,
}

impl LinkQueue {
    pub const MAX_LINKS: usize = 4;

    pub const fn new() -> Self {
        Self { links: Vec::new() }
    }

    /// False when the link was dropped.
    pub fn push(&mut self, raw: &str) -> bool {
        if raw.len() > MAX_LINK_LEN || self.links.len() >= Self::MAX_LINKS {
            return false;
        }
        self.links.push(raw.to_string());
        true
    }

    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.links)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44";

    #[test]
    fn the_three_allowed_forms_parse() {
        assert_eq!(parse("dynodoc://open/workspace"), Ok(Target::Workspace));
        assert_eq!(
            parse(&format!("dynodoc://open/projects/{ID}")),
            Ok(Target::Project(ID.to_string()))
        );
        assert_eq!(
            parse(&format!("dynodoc://open/documents/{ID}")),
            Ok(Target::Document(ID.to_string()))
        );
        assert_eq!(Target::Workspace.path(), "/workspace");
        assert_eq!(Target::Project(ID.into()).path(), format!("/projects/{ID}"));
        assert_eq!(
            Target::Document(ID.into()).path(),
            format!("/documents/{ID}")
        );
    }

    #[test]
    fn case_and_one_trailing_slash_are_tolerated_and_normalised() {
        assert_eq!(parse("DynoDoc://open/workspace/"), Ok(Target::Workspace));
        let upper = ID.to_uppercase();
        assert_eq!(
            parse(&format!("dynodoc://open/projects/{upper}/")),
            Ok(Target::Project(ID.to_string()))
        );
    }

    #[test]
    fn other_schemes_and_other_apps_are_not_ours() {
        for bad in [
            "https://app.dynodoc.online/workspace",
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "dynodoc:open/workspace",
            "dynodoc:/open/workspace",
            "dynodoc-evil://open/workspace",
            "xdynodoc://open/workspace",
            "",
            "dynodoc",
            "dynodoc:/",
        ] {
            assert_eq!(parse(bad), Err(LinkError::NotOurs), "{bad:?}");
        }
    }

    #[test]
    fn every_other_path_is_refused() {
        for bad in [
            "dynodoc://open/",
            "dynodoc://open",
            "dynodoc://",
            "dynodoc://open/admin",
            "dynodoc://open/profile",
            "dynodoc://open/auth/verify",
            "dynodoc://open/login",
            "dynodoc://open/workspace/extra",
            "dynodoc://open/projects",
            "dynodoc://open/projects/",
            "dynodoc://open/projects/someone-else",
            "dynodoc://workspace",
            "dynodoc://projects/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44",
            "dynodoc://evil.example/workspace",
            "dynodoc://open/projects/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44/files",
            "dynodoc://open/documents/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44/edit",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tokens_servers_ports_and_credentials_cannot_ride_along() {
        let with = |tail: &str| format!("dynodoc://open/projects/{ID}{tail}");
        for bad in [
            with("?token=abc"),
            with("#frag"),
            with("?next=https://evil.example"),
            with("%2f..%2f..%2fadmin"),
            with("/../../admin"),
            with("/.."),
            with("//"),
            with(";x=1"),
            with("\\..\\admin"),
            with(" "),
            with("\n"),
            with("\0"),
            "dynodoc://user@open/workspace".to_string(),
            "dynodoc://open:8080/workspace".to_string(),
            "dynodoc://open@evil.example/workspace".to_string(),
            "dynodoc://open.evil.example/workspace".to_string(),
            "dynodoc://open/workspace?server=https://evil.example".to_string(),
            "dynodoc://https://evil.example/workspace".to_string(),
            "dynodoc://open//workspace".to_string(),
            "dynodoc:///open/workspace".to_string(),
            "dynodoc://open\\workspace".to_string(),
            "dynodoc://open/workspace%00".to_string(),
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn identifiers_must_be_uuids() {
        for bad in [
            "0b0f6a52",
            "0b0f6a526a0e4c1f9d575b7a3a1e2c44",
            "{0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44}",
            "urn:uuid:0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44",
            "0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c4",
            "0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c444",
            "0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c4g",
            "0b0f6a52_6a0e_4c1f_9d57_5b7a3a1e2c44",
            "..",
            "workspace",
        ] {
            assert!(
                parse(&format!("dynodoc://open/projects/{bad}")).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn long_input_is_refused_before_anything_else() {
        let long = format!("dynodoc://open/projects/{}", "a".repeat(100_000));
        assert_eq!(parse(&long), Err(LinkError::TooLong));
        assert_eq!(
            parse(&"d".repeat(MAX_LINK_LEN + 1)),
            Err(LinkError::TooLong)
        );
        // Non-ASCII near the prefix boundary must not panic on a char boundary.
        assert!(parse("dynodoc:/é/open/workspace").is_err());
        assert!(parse("dynodocé//open").is_err());
        assert!(parse("ｄynodoc://open/workspace").is_err());
    }

    /// The accepted set, stated exactly, as a whitelist. Every string built from this
    /// vocabulary (up to four segments joined by slashes, after five spellings of the scheme,
    /// with and without a trailing slash) is parsed; the set of strings that are accepted must
    /// be EXACTLY the twenty listed by hand below. The tests above name refused inputs; this
    /// one fails if any other shape becomes accepted.
    #[test]
    fn exactly_three_path_shapes_are_accepted() {
        use std::collections::BTreeSet;
        let upper = ID.to_uppercase();
        let words: Vec<&str> = vec![
            "open",
            "workspace",
            "projects",
            "documents",
            "admin",
            "login",
            "auth",
            ID,
            &upper,
            "..",
            ".",
            "",
            "x",
        ];
        let mut accepted: BTreeSet<String> = BTreeSet::new();
        let mut tried = 0usize;
        for depth in 0..=4usize {
            let mut indexes = vec![0usize; depth];
            loop {
                let parts: Vec<&str> = indexes.iter().map(|&i| words[i]).collect();
                for scheme in [
                    "dynodoc://",
                    "DYNODOC://",
                    "dynodoc:/",
                    "dynodoc:",
                    "dynodoc",
                    "",
                ] {
                    for trailing in ["", "/"] {
                        let text = format!("{scheme}{}{trailing}", parts.join("/"));
                        tried += 1;
                        if let Ok(target) = parse(&text) {
                            // Whatever is accepted maps to one of three server paths.
                            let path = target.path();
                            assert!(
                                path == "/workspace"
                                    || path == format!("/projects/{ID}")
                                    || path == format!("/documents/{ID}"),
                                "{text:?} -> {path}"
                            );
                            accepted.insert(text);
                        }
                    }
                }
                // Advance the odometer.
                let mut at = 0;
                while at < depth {
                    indexes[at] += 1;
                    if indexes[at] < words.len() {
                        break;
                    }
                    indexes[at] = 0;
                    at += 1;
                }
                if at == depth {
                    break;
                }
            }
        }
        let mut expected: BTreeSet<String> = BTreeSet::new();
        for scheme in ["dynodoc://", "DYNODOC://"] {
            for trailing in ["", "/"] {
                expected.insert(format!("{scheme}open/workspace{trailing}"));
                for id in [ID, upper.as_str()] {
                    expected.insert(format!("{scheme}open/projects/{id}{trailing}"));
                    expected.insert(format!("{scheme}open/documents/{id}{trailing}"));
                }
            }
        }
        assert_eq!(expected.len(), 20);
        assert!(tried > 300_000, "tried only {tried}");
        assert_eq!(accepted, expected);
    }

    #[test]
    fn links_that_arrive_early_are_kept_briefly_and_bounded() {
        let mut queue = LinkQueue::default();
        assert!(queue.drain().is_empty());
        for i in 0..LinkQueue::MAX_LINKS {
            assert!(queue.push(&format!("dynodoc://open/workspace#{i}")));
        }
        assert!(!queue.push("dynodoc://open/workspace"), "full");
        assert!(
            !LinkQueue::default().push(&"d".repeat(MAX_LINK_LEN + 1)),
            "too long"
        );
        assert_eq!(queue.drain().len(), LinkQueue::MAX_LINKS);
        assert!(queue.drain().is_empty(), "drained once");
        assert!(queue.push("dynodoc://open/workspace"), "room again");
    }
}
