//! `dynodoc://` links. The operating system hands the raw text of any such link to the app,
//! whoever wrote it (a web page, a mail, a chat), so it is treated as hostile input.
//!
//! Accepted forms, and nothing else:
//!
//! ```text
//! dynodoc://open/workspace
//! dynodoc://open/projects/<uuid>
//! dynodoc://open/documents/<uuid>
//! dynodoc://signin?code=<code>&state=<state>
//! ```
//!
//! The first three give a path on the CONFIGURED server (`/workspace`, `/projects/<uuid>`,
//! `/documents/<uuid>`). A link never names a server, a port or credentials, and the first
//! three carry no query or fragment at all, so there is nothing to leak or redirect.
//!
//! The fourth is the end of the browser sign-in (docs/decisions/017): it carries a one-time
//! hand-over CODE and the app's own STATE, in that order, each once, and nothing else. The code
//! is not a session and not a token: it is useless without the verifier that never leaves this
//! app, it works once, and it expires after a minute. `signin.rs` checks the state against the
//! attempt this app started and decides what happens next; this module only reads the shape.

use std::fmt;

/// The scheme, as written in tauri.conf.json (a test compares them).
#[cfg(test)]
pub const SCHEME: &str = "dynodoc";
const PREFIX: &str = "dynodoc://";
/// Far longer than any accepted link (the longest, a sign-in link, is 158 characters).
pub const MAX_LINK_LEN: usize = 256;

/// What an accepted link asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Workspace,
    Project,
    Document,
    SignIn,
}

/// The characters a query value may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// Lower-case hexadecimal digits.
    Hex,
    /// Letters, digits, hyphen and underscore (base64url).
    UrlSafe,
}

impl Charset {
    fn allows(self, b: u8) -> bool {
        match self {
            Self::Hex => matches!(b, b'0'..=b'9' | b'a'..=b'f'),
            Self::UrlSafe => b.is_ascii_alphanumeric() || b == b'-' || b == b'_',
        }
    }
}

/// One query value a shape requires: its name, the bounds of its length and its characters.
#[derive(Debug, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub min: usize,
    pub max: usize,
    pub charset: Charset,
}

/// One accepted shape: these fixed words, then (when `takes_id`) a UUID, then (when `query` is
/// not empty) exactly these query values, each once, in this order. A shape with no `query`
/// accepts no query at all.
#[derive(Debug, PartialEq, Eq)]
pub struct Shape {
    pub words: &'static [&'static str],
    pub takes_id: bool,
    pub query: &'static [Param],
    pub kind: Kind,
}

/// THE list of accepted links. `parse` accepts a link if and only if it matches a row here; no
/// other code names a path, so a link shape cannot be added without changing this table, and a
/// test compares the table with the documented list above.
///
/// The sign-in row: the code is exactly what the API makes (64 lower-case hexadecimal digits);
/// the state is the app's own random value, 16 to 64 base64url characters (the API takes the
/// same bounds).
pub const SHAPES: &[Shape] = &[
    Shape {
        words: &["open", "workspace"],
        takes_id: false,
        query: &[],
        kind: Kind::Workspace,
    },
    Shape {
        words: &["open", "projects"],
        takes_id: true,
        query: &[],
        kind: Kind::Project,
    },
    Shape {
        words: &["open", "documents"],
        takes_id: true,
        query: &[],
        kind: Kind::Document,
    },
    Shape {
        words: &["signin"],
        takes_id: false,
        query: &[
            Param {
                name: "code",
                min: 64,
                max: 64,
                charset: Charset::Hex,
            },
            Param {
                name: "state",
                min: 16,
                max: 64,
                charset: Charset::UrlSafe,
            },
        ],
        kind: Kind::SignIn,
    },
];

/// What a link asks the app to do. `Debug` never shows the code or the state.
#[derive(Clone, PartialEq, Eq)]
pub enum Target {
    Workspace,
    Project(String),
    Document(String),
    SignIn { code: String, state: String },
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace => f.write_str("Workspace"),
            Self::Project(id) => f.debug_tuple("Project").field(id).finish(),
            Self::Document(id) => f.debug_tuple("Document").field(id).finish(),
            Self::SignIn { .. } => f.write_str("SignIn { code: .., state: .. }"),
        }
    }
}

impl Target {
    /// The server-relative path of a link that opens a page, e.g. `/projects/0b0f...`. A
    /// sign-in link opens no page of its own: `None`.
    pub fn path(&self) -> Option<String> {
        match self {
            Self::Workspace => Some("/workspace".to_string()),
            Self::Project(id) => Some(format!("/projects/{id}")),
            Self::Document(id) => Some(format!("/documents/{id}")),
            Self::SignIn { .. } => None,
        }
    }

    fn of(kind: Kind, id: Option<String>, values: Vec<String>) -> Option<Self> {
        match (kind, id, <[String; 2]>::try_from(values)) {
            (Kind::Workspace, None, Err(v)) if v.is_empty() => Some(Self::Workspace),
            (Kind::Project, Some(id), Err(v)) if v.is_empty() => Some(Self::Project(id)),
            (Kind::Document, Some(id), Err(v)) if v.is_empty() => Some(Self::Document(id)),
            (Kind::SignIn, None, Ok([code, state])) => Some(Self::SignIn { code, state }),
            _ => None,
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

// This function contains no string literal on purpose: every word it compares comes from
// SHAPES (a test fails if one is added here).
pub fn parse(raw: &str) -> Result<Target, LinkError> {
    if raw.len() > MAX_LINK_LEN {
        return Err(LinkError::TooLong);
    }
    let rest = match raw.get(..PREFIX.len()) {
        Some(head) if head.eq_ignore_ascii_case(PREFIX) => &raw[PREFIX.len()..],
        _ => return Err(LinkError::NotOurs),
    };
    // Letters, digits, hyphen and slash in the path, then at most one question mark and a
    // query of letters, digits, hyphen, underscore, equals and ampersand. That leaves out every
    // character the parsers of URLs disagree about: percent, backslash, hash, at, colon, dot,
    // semicolon, spaces and controls.
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    // One trailing slash is how some systems hand the link over (after the query, if any).
    let (path, query) = match query {
        Some(query) => (path, Some(query.strip_suffix('/').unwrap_or(query))),
        None => (path.strip_suffix('/').unwrap_or(path), None),
    };
    let path_ok = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'/');
    let query_ok = query.is_none_or(|q| {
        q.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'=' | b'&'))
    });
    if !path_ok || !query_ok {
        return Err(LinkError::Malformed);
    }
    let parts: Vec<&str> = path.split('/').collect();
    for shape in SHAPES {
        let fixed = shape.words.len();
        if parts.len() == fixed + usize::from(shape.takes_id)
            && parts[..fixed] == *shape.words
            && shape.query.is_empty() == query.is_none()
        {
            let id = if shape.takes_id {
                Some(uuid(parts[fixed])?)
            } else {
                None
            };
            let values = match query {
                Some(query) => query_values(query, shape.query)?,
                None => Vec::new(),
            };
            return Target::of(shape.kind, id, values).ok_or(LinkError::NotAllowed);
        }
    }
    if parts.iter().any(|part| part.is_empty()) {
        Err(LinkError::Malformed)
    } else {
        Err(LinkError::NotAllowed)
    }
}

/// The values of `name=value&name=value`, which must be exactly the shape's names in its
/// order, each value within its bounds and characters.
fn query_values(query: &str, wanted: &[Param]) -> Result<Vec<String>, LinkError> {
    let pairs: Vec<&str> = query.split('&').collect();
    if pairs.len() != wanted.len() {
        return Err(LinkError::Malformed);
    }
    let mut values = Vec::with_capacity(wanted.len());
    for (pair, param) in pairs.iter().zip(wanted) {
        let (name, value) = pair.split_once('=').ok_or(LinkError::Malformed)?;
        let fits = (param.min..=param.max).contains(&value.len())
            && value.bytes().all(|b| param.charset.allows(b));
        if name != param.name || !fits {
            return Err(LinkError::Malformed);
        }
        values.push(value.to_string());
    }
    Ok(values)
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44";
    /// A code as the API makes it, and a state as the app makes it.
    const CODE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const STATE: &str = "b3JkZXJseS1zdGF0ZS12YWx1ZQ";

    fn sign_in_link() -> String {
        format!("dynodoc://signin?code={CODE}&state={STATE}")
    }

    #[test]
    fn the_four_allowed_forms_parse() {
        assert_eq!(parse("dynodoc://open/workspace"), Ok(Target::Workspace));
        assert_eq!(
            parse(&format!("dynodoc://open/projects/{ID}")),
            Ok(Target::Project(ID.to_string()))
        );
        assert_eq!(
            parse(&format!("dynodoc://open/documents/{ID}")),
            Ok(Target::Document(ID.to_string()))
        );
        assert_eq!(
            parse(&sign_in_link()),
            Ok(Target::SignIn {
                code: CODE.to_string(),
                state: STATE.to_string()
            })
        );
        assert_eq!(Target::Workspace.path().as_deref(), Some("/workspace"));
        assert_eq!(
            Target::Project(ID.into()).path(),
            Some(format!("/projects/{ID}"))
        );
        assert_eq!(
            Target::Document(ID.into()).path(),
            Some(format!("/documents/{ID}"))
        );
        // A sign-in link opens no page of its own.
        assert_eq!(parse(&sign_in_link()).unwrap().path(), None);
    }

    #[test]
    fn case_and_one_trailing_slash_are_tolerated_and_normalised() {
        assert_eq!(parse("DynoDoc://open/workspace/"), Ok(Target::Workspace));
        let upper = ID.to_uppercase();
        assert_eq!(
            parse(&format!("dynodoc://open/projects/{upper}/")),
            Ok(Target::Project(ID.to_string()))
        );
        // Windows hands a custom-scheme link over with a slash added at the very end, after
        // the query. The scheme may be written in capitals; the values may not change case.
        let expected = Ok(Target::SignIn {
            code: CODE.to_string(),
            state: STATE.to_string(),
        });
        assert_eq!(parse(&format!("{}/", sign_in_link())), expected);
        assert_eq!(
            parse(&format!("DYNODOC://signin?code={CODE}&state={STATE}/")),
            expected
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
        // A sign-in link under another scheme is just as foreign.
        for scheme in ["https", "dynodoc-evil", "xdynodoc", "dynodoc:", "dynodoc:/"] {
            let tail = format!("signin?code={CODE}&state={STATE}");
            assert_eq!(
                parse(&format!("{scheme}://{tail}")),
                Err(LinkError::NotOurs)
            );
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
            "dynodoc://open/settings",
            "dynodoc://open/workspace/extra",
            "dynodoc://open/projects",
            "dynodoc://open/projects/",
            "dynodoc://open/projects/someone-else",
            "dynodoc://workspace",
            "dynodoc://projects/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44",
            "dynodoc://evil.example/workspace",
            "dynodoc://open/projects/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44/files",
            "dynodoc://open/documents/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44/edit",
            // The sign-in link has no path under open, and no path after it.
            "dynodoc://open/signin",
            "dynodoc://signin/extra",
            "dynodoc://signin/",
            "dynodoc://signin",
            "dynodoc://signin/open/workspace",
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
            // The three page links take no query at all, not even a valid sign-in one.
            format!("dynodoc://open/workspace?code={CODE}&state={STATE}"),
            format!("dynodoc://open/projects/{ID}?code={CODE}&state={STATE}"),
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn hostile_sign_in_links_are_refused() {
        let base = format!("dynodoc://signin?code={CODE}&state={STATE}");
        let long_state = "s".repeat(65);
        let short_state = "s".repeat(15);
        let upper = CODE.to_uppercase();
        let mut cases: Vec<String> = vec![
            // Anything beyond the two values, before, between or after.
            format!("{base}&x=1"),
            format!("{base}&code={CODE}"),
            format!("{base}&state={STATE}"),
            format!("dynodoc://signin?x=1&code={CODE}&state={STATE}"),
            format!("dynodoc://signin?code={CODE}&x=1&state={STATE}"),
            format!("dynodoc://signin?code={CODE}"),
            format!("dynodoc://signin?state={STATE}"),
            // The order is the table's.
            format!("dynodoc://signin?state={STATE}&code={CODE}"),
            // Separators and empties.
            format!("dynodoc://signin?code={CODE}&&state={STATE}"),
            format!("dynodoc://signin?code={CODE};state={STATE}"),
            format!("dynodoc://signin?&code={CODE}&state={STATE}"),
            format!("dynodoc://signin??code={CODE}&state={STATE}"),
            format!("{base}?"),
            format!("{base}&"),
            "dynodoc://signin?".to_string(),
            "dynodoc://signin?code=&state=".to_string(),
            format!("dynodoc://signin?code={CODE}&state="),
            format!("dynodoc://signin?code=&state={STATE}"),
            format!("dynodoc://signin?code{CODE}&state={STATE}"),
            format!("dynodoc://signin?code=={CODE}&state={STATE}"),
            format!("dynodoc://signin?code={CODE}=&state={STATE}"),
            // A fragment, encodings, controls and spaces.
            format!("{base}#x"),
            format!("dynodoc://signin#x?code={CODE}&state={STATE}"),
            format!("dynodoc://signin?code={CODE}%41&state={STATE}"),
            format!("dynodoc://signin?code={CODE}&state={STATE}%0a"),
            format!("dynodoc://signin?code={CODE}&state={STATE}\n"),
            format!("dynodoc://signin?code={CODE}&state={STATE} "),
            format!("dynodoc://signin?code={CODE}&state={STATE}\0"),
            format!("dynodoc://signin?code={CODE}&state={STATE}\\x"),
            format!("dynodoc://signin?code={CODE}&state={STATE}/x"),
            format!("dynodoc://signin?code={CODE}&state={STATE}//"),
            format!("dynodoc://signin?code={CODE}&state={STATE}."),
            format!("dynodoc://signin?code={CODE}&state={STATE}@evil.example"),
            format!("dynodoc://signin?code={CODE}&state={STATE}:80"),
            // Names are exact.
            format!("dynodoc://signin?Code={CODE}&state={STATE}"),
            format!("dynodoc://signin?code={CODE}&State={STATE}"),
            format!("dynodoc://signin?codes={CODE}&state={STATE}"),
            format!("dynodoc://signin?token={CODE}&state={STATE}"),
            format!("dynodoc://signin?code={CODE}&next={STATE}"),
            // Values: length and characters.
            format!("dynodoc://signin?code={upper}&state={STATE}"),
            format!("dynodoc://signin?code={}&state={STATE}", &CODE[1..]),
            format!("dynodoc://signin?code={CODE}0&state={STATE}"),
            format!("dynodoc://signin?code={}g&state={STATE}", &CODE[1..]),
            format!("dynodoc://signin?code={}-&state={STATE}", &CODE[1..]),
            format!("dynodoc://signin?code={CODE}&state={long_state}"),
            format!("dynodoc://signin?code={CODE}&state={short_state}"),
            format!("dynodoc://signin?code={CODE}&state={}+", &STATE[1..]),
            format!("dynodoc://signin?code={CODE}&state={}.", &STATE[1..]),
            // Where it sits.
            format!("dynodoc://open/signin?code={CODE}&state={STATE}"),
            format!("dynodoc://signin/?code={CODE}&state={STATE}"),
            format!("dynodoc://signin/x?code={CODE}&state={STATE}"),
            format!("dynodoc://evil.example/signin?code={CODE}&state={STATE}"),
            format!("dynodoc://user@signin?code={CODE}&state={STATE}"),
            format!("dynodoc://signin:8080?code={CODE}&state={STATE}"),
            format!("dynodoc://signin.evil.example?code={CODE}&state={STATE}"),
            format!("dynodoc:///signin?code={CODE}&state={STATE}"),
            format!("dynodoc://sign-in?code={CODE}&state={STATE}"),
            format!("dynodoc://SIGNIN?code={CODE}&state={STATE}"),
        ];
        // Non-ASCII near every position.
        cases.push(format!("dynodoc://signin?code={CODE}&state={STATE}é"));
        cases.push(format!(
            "dynodoc://signin?code=é{}&state={STATE}",
            &CODE[1..]
        ));
        for bad in cases {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
        // And two long inputs are refused before anything else.
        assert_eq!(
            parse(&format!("{base}{}", "a".repeat(10_000))),
            Err(LinkError::TooLong)
        );
        assert_eq!(
            parse(&format!("{base}&{}", "a".repeat(300))),
            Err(LinkError::TooLong)
        );
    }

    #[test]
    fn the_longest_sign_in_link_still_fits() {
        let longest = format!("dynodoc://signin?code={CODE}&state={}/", "s".repeat(64));
        assert!(longest.len() <= MAX_LINK_LEN, "{}", longest.len());
        assert!(parse(&longest).is_ok());
        // And the limit leaves room: nothing accepted is near it.
        assert!(longest.len() * 2 > MAX_LINK_LEN);
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

    #[test]
    fn a_link_never_shows_its_secrets_when_printed() {
        let target = parse(&sign_in_link()).unwrap();
        for text in [format!("{target:?}"), format!("{target:#?}")] {
            assert!(!text.contains(CODE), "{text}");
            assert!(!text.contains(STATE), "{text}");
            assert!(!text.contains(&CODE[..16]), "{text}");
        }
        let refused = format!("{:?}", parse(&format!("{}&x=1", sign_in_link())));
        assert!(
            !refused.contains(CODE) && !refused.contains(STATE),
            "{refused}"
        );
    }

    /// The accepted set of the three page links, stated exactly, as a whitelist. Every string
    /// built from this vocabulary (up to four segments joined by slashes, after five spellings
    /// of the scheme, with and without a trailing slash) is parsed; the set of strings that are
    /// accepted must be EXACTLY the twenty listed by hand below. The tests above name refused
    /// inputs; this one fails if any other shape becomes accepted.
    #[test]
    fn exactly_three_path_shapes_are_accepted_without_a_query() {
        use std::collections::BTreeSet;
        let upper = ID.to_uppercase();
        let words: Vec<&str> = vec![
            "open",
            "workspace",
            "projects",
            "documents",
            "signin",
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
                                path.as_deref() == Some("/workspace")
                                    || path == Some(format!("/projects/{ID}"))
                                    || path == Some(format!("/documents/{ID}")),
                                "{text:?} -> {path:?}"
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
        // The expected set comes from the table, rendered by hand here (not by parse): each
        // shape without a query, both spellings of the scheme, with and without a trailing slash.
        let mut expected: BTreeSet<String> = BTreeSet::new();
        for scheme in ["dynodoc://", "DYNODOC://"] {
            for trailing in ["", "/"] {
                for shape in SHAPES.iter().filter(|s| s.query.is_empty()) {
                    let words = shape.words.join("/");
                    if shape.takes_id {
                        for id in [ID, upper.as_str()] {
                            expected.insert(format!("{scheme}{words}/{id}{trailing}"));
                        }
                    } else {
                        expected.insert(format!("{scheme}{words}{trailing}"));
                    }
                }
            }
        }
        assert_eq!(expected.len(), 20, "the three documented shapes");
        assert!(tried > 400_000, "tried only {tried}");
        assert_eq!(accepted, expected);
    }

    /// The same for the sign-in link. Every string made of a path, a query built from up to three
    /// `name=value` pairs (valid and invalid names and values), a trailing slash and a scheme is
    /// parsed, and the accepted set must be EXACTLY the twenty-four listed by hand: the sign-in
    /// path, one question mark, `code=` a 64-digit lower-case hexadecimal value, `&`, `state=` a
    /// value of 16 to 64 base64url characters (six are tried: three lengths and three more that
    /// happen to be hexadecimal text or its upper-case or shortened form, which are valid
    /// states), and nothing after, except one slash.
    #[test]
    fn exactly_one_query_shape_is_accepted() {
        use std::collections::BTreeSet;
        let codes = [
            CODE.to_string(),
            CODE.to_uppercase(),
            CODE[1..].to_string(),
            format!("{CODE}0"),
        ];
        // Valid as a state: 16 to 64 of letters, digits, hyphen and underscore. Note that the
        // code's own spelling, its upper-case spelling and its 63-digit cut are among them.
        let valid_states = [
            "a".repeat(16),
            "Zz_-09".repeat(4),
            "g".repeat(64),
            CODE.to_string(),
            CODE.to_uppercase(),
            CODE[1..].to_string(),
        ];
        let invalid_states = ["a".repeat(15), format!("{}+", "a".repeat(20))];
        let values: BTreeSet<&String> = codes
            .iter()
            .chain(valid_states.iter())
            .chain(invalid_states.iter())
            .collect();
        let mut pairs: Vec<String> = Vec::new();
        for name in ["code", "state", "x"] {
            for value in &values {
                pairs.push(format!("{name}={value}"));
            }
        }
        let mut queries: Vec<String> = vec![String::new()];
        for first in &pairs {
            queries.push(first.clone());
            for second in &pairs {
                queries.push(format!("{first}&{second}"));
                for third in &pairs {
                    queries.push(format!("{first}&{second}&{third}"));
                }
            }
        }
        // Odd tails the generator cannot make.
        for odd in [
            format!("{}&", pairs[0]),
            format!("&{}", pairs[0]),
            format!("code={CODE}&&state={}", valid_states[0]),
            format!("code={CODE};state={}", valid_states[0]),
            format!("?code={CODE}&state={}", valid_states[0]),
            format!("code={CODE}&state={}?", valid_states[0]),
            format!("code={CODE}&state={}#f", valid_states[0]),
            format!("code={CODE}&state={}%41", valid_states[0]),
        ] {
            queries.push(odd);
        }
        let mut accepted: BTreeSet<String> = BTreeSet::new();
        let mut tried = 0usize;
        for scheme in ["dynodoc://", "DYNODOC://"] {
            for path in ["signin", "signin/", "open/workspace"] {
                for question in ["?", ""] {
                    for query in &queries {
                        for trailing in ["", "/"] {
                            let text = format!("{scheme}{path}{question}{query}{trailing}");
                            tried += 1;
                            if let Ok(Target::SignIn { code, state }) = parse(&text) {
                                assert_eq!(code, CODE, "{text:?}");
                                assert!(valid_states.contains(&state), "{text:?}");
                                accepted.insert(text);
                            } else if let Ok(other) = parse(&text) {
                                // The page links accept no query: only when there is none.
                                assert!(
                                    question.is_empty() && query.is_empty(),
                                    "{text:?} -> {other:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        let mut expected: BTreeSet<String> = BTreeSet::new();
        for scheme in ["dynodoc://", "DYNODOC://"] {
            for state in &valid_states {
                for trailing in ["", "/"] {
                    expected.insert(format!(
                        "{scheme}signin?code={CODE}&state={state}{trailing}"
                    ));
                }
            }
        }
        assert_eq!(expected.len(), 24);
        assert!(tried > 400_000, "tried only {tried}");
        assert_eq!(accepted, expected);
    }

    /// The table, stated again by hand. Adding, removing or changing a row fails here, so the
    /// documented list (module comment, README) and the code cannot drift apart.
    #[test]
    fn the_table_is_exactly_the_documented_list() {
        type Row = (String, bool, Vec<(String, usize, usize, Charset)>, Kind);
        let rows: Vec<Row> = SHAPES
            .iter()
            .map(|s| {
                (
                    s.words.join("/"),
                    s.takes_id,
                    s.query
                        .iter()
                        .map(|p| (p.name.to_string(), p.min, p.max, p.charset))
                        .collect(),
                    s.kind,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("open/workspace".to_string(), false, vec![], Kind::Workspace),
                ("open/projects".to_string(), true, vec![], Kind::Project),
                ("open/documents".to_string(), true, vec![], Kind::Document),
                (
                    "signin".to_string(),
                    false,
                    vec![
                        ("code".to_string(), 64, 64, Charset::Hex),
                        ("state".to_string(), 16, 64, Charset::UrlSafe),
                    ],
                    Kind::SignIn
                ),
            ]
        );
        // Every kind has a documented shape and a path, by an exhaustive match: a new Kind does
        // not compile until it is dealt with here.
        for kind in [Kind::Workspace, Kind::Project, Kind::Document, Kind::SignIn] {
            let (documented, takes_id, path) = match kind {
                Kind::Workspace => ("open/workspace", false, Some("/workspace")),
                Kind::Project => ("open/projects", true, Some("/projects/")),
                Kind::Document => ("open/documents", true, Some("/documents/")),
                Kind::SignIn => ("signin", false, None),
            };
            let row = SHAPES
                .iter()
                .find(|s| s.kind == kind)
                .expect("a row per kind");
            assert_eq!(row.words.join("/"), documented);
            assert_eq!(row.takes_id, takes_id);
            let values = if row.query.is_empty() {
                Vec::new()
            } else {
                vec![CODE.to_string(), STATE.to_string()]
            };
            let target = Target::of(kind, takes_id.then(|| ID.to_string()), values).unwrap();
            match path {
                Some(prefix) => assert!(target.path().unwrap().starts_with(prefix)),
                None => assert_eq!(target.path(), None),
            }
        }
        assert_eq!(SHAPES.len(), 4);
        // A kind given the wrong kind of id or values is not a target.
        assert!(Target::of(Kind::Workspace, Some(ID.into()), vec![]).is_none());
        assert!(Target::of(Kind::Project, None, vec![]).is_none());
        assert!(Target::of(Kind::Workspace, None, vec![CODE.into(), STATE.into()]).is_none());
        assert!(Target::of(Kind::SignIn, None, vec![]).is_none());
        assert!(Target::of(Kind::SignIn, None, vec![CODE.into()]).is_none());
        assert!(Target::of(
            Kind::SignIn,
            Some(ID.into()),
            vec![CODE.into(), STATE.into()]
        )
        .is_none());
        assert!(PREFIX.eq_ignore_ascii_case(&format!("{SCHEME}://")));
        // The module documentation lists the same four links.
        let source = include_str!("deeplink.rs");
        let source = &source[..source.find("mod tests {").unwrap()];
        for line in [
            "//! dynodoc://open/workspace",
            "//! dynodoc://open/projects/<uuid>",
            "//! dynodoc://open/documents/<uuid>",
            "//! dynodoc://signin?code=<code>&state=<state>",
        ] {
            assert!(source.contains(line), "{line}");
        }
    }

    /// `parse` takes every word from the table: no string literal in it, so a shape cannot be
    /// slipped in beside the table (the way an extra match arm once could).
    #[test]
    fn parse_names_no_path_of_its_own() {
        let source = include_str!("deeplink.rs");
        let start = source.find("pub fn parse(").unwrap();
        let end = source[start..]
            .find(
                "
}
",
            )
            .unwrap()
            + start;
        let body: String = source[start..end]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        assert!(!body.contains('"'), "string literal in parse: {body}");
        // And nothing else in the production code decides what a link may be.
        let production = &source[..source.find("mod tests {").unwrap()];
        for word in ["settings", "admin", "login", "auth"] {
            assert!(!production.contains(word), "{word}");
        }
        assert_eq!(production.matches("pub fn parse(").count(), 1);
        assert_eq!(
            production.matches("Target::of(").count(),
            1,
            "one place builds a target from a row"
        );
        assert_eq!(
            production.matches("query_values(").count(),
            2,
            "one definition, one call"
        );
    }
}
