//! The shapes the agent layer passes around. A key has its own type with no `Debug`, `Clone`,
//! `Serialize` or `Display`, so it cannot be logged or sent to the page by accident.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use zeroize::Zeroizing;

/// The providers the app can call. Everything but Anthropic and Google speaks the OpenAI chat
/// completions wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Anthropic,
    Openai,
    Google,
    Mistral,
    Deepseek,
    Qwen,
    Kimi,
    Glm,
    Ollama,
    /// Any server that speaks the OpenAI chat completions format, at an address the person gives.
    Compat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Anthropic,
    Gemini,
    OpenAi,
}

impl Kind {
    pub fn style(self) -> Style {
        match self {
            Kind::Anthropic => Style::Anthropic,
            Kind::Google => Style::Gemini,
            _ => Style::OpenAi,
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Kind::Ollama)
    }

    /// Whether to ask for the usage chunk at the end of a stream (providers that document the
    /// option; the others send usage anyway, or not at all).
    pub fn asks_for_usage(self) -> bool {
        matches!(
            self,
            Kind::Openai | Kind::Deepseek | Kind::Qwen | Kind::Ollama | Kind::Compat
        )
    }
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub kind: Kind,
    pub base: Url,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    NotAnAddress,
    Credentials,
    Insecure,
    BadModel,
}

impl std::fmt::Display for EndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAnAddress => "The provider's address is not a web address.",
            Self::Credentials => {
                "A provider's address has no user name, password, query or fragment."
            }
            Self::Insecure => {
                "Plain http is only for a server on this computer; use https for any other."
            }
            Self::BadModel => "A model id uses letters, digits and . _ : / @ + - only.",
        })
    }
}

pub fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1"
}

impl Endpoint {
    pub fn parse(kind: Kind, base: &str, model: &str) -> Result<Self, EndpointError> {
        let url = Url::parse(base.trim()).map_err(|_| EndpointError::NotAnAddress)?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(EndpointError::NotAnAddress);
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(EndpointError::Credentials);
        }
        if url.scheme() == "http" && !is_loopback_host(url.host_str().unwrap_or("")) {
            return Err(EndpointError::Insecure);
        }
        let ok = !model.is_empty()
            && model.len() <= 200
            && model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b));
        if !ok {
            return Err(EndpointError::BadModel);
        }
        Ok(Self {
            kind,
            base: url,
            model: model.to_string(),
        })
    }

    /// `host` or `host:port`, as the consent names it.
    pub fn host(&self) -> String {
        match self.base.port() {
            Some(p) => format!("{}:{p}", self.base.host_str().unwrap_or("")),
            None => self.base.host_str().unwrap_or("").to_string(),
        }
    }
}

/// An API key. No `Debug`, `Clone`, `Display` or `Serialize`: it can only be read through `expose`
/// at the one place a request header is made.
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    pub fn new(text: &str) -> Option<Self> {
        let t = text.trim();
        let ok = (8..=512).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_graphic());
        ok.then(|| Self(Zeroizing::new(t.to_string())))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The last four characters, only when the key is long enough that they do not give it away.
    pub fn hint(&self) -> String {
        let k = self.expose();
        if k.len() >= 12 {
            k[k.len() - 4..].to_string()
        } else {
            "····".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDecl {
    /// The tool's name in Dynodoc's table (`history.read`).
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// The name a provider sees: letters, digits, `_` and `-` only (providers differ on dots).
pub fn wire_name(name: &str) -> String {
    name.replace('.', "__")
}

pub fn from_wire_name(name: &str) -> String {
    name.replace("__", ".")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    /// Dynodoc's name for the tool.
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    User(String),
    Assistant {
        text: String,
        calls: Vec<ToolCall>,
        /// The provider's own record of the turn, when it needs it back (Gemini's thought
        /// signatures travel with the function calls).
        raw: Option<Value>,
    },
    ToolResult {
        call_id: String,
        name: String,
        output: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    End,
    ToolCalls,
    Length,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub usage: Usage,
    pub stop: Stop,
    pub raw: Option<Value>,
}

/// What can go wrong with a call, in words a person can act on. Never holds a key, a header, a
/// request body or an address with a query.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentError {
    /// The provider answered with an error status.
    Provider {
        status: u16,
        message: String,
    },
    Rejected(String),
    RateLimited {
        retry_after_ms: Option<u64>,
    },
    Unreachable,
    TimedOut,
    Cancelled,
    TooLarge,
    BadAnswer(String),
    NoKey,
    NoConsent,
    Offline,
    CapReached(String),
    BadSetup(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider { status, message } => write!(f, "The provider answered {status}: {message}"),
            Self::Rejected(m) => write!(f, "The provider refused the request: {m}"),
            Self::RateLimited { .. } => f.write_str("The provider says there are too many requests. Try again in a moment."),
            Self::Unreachable => f.write_str("The provider could not be reached. Check the connection and the address in Settings, Providers."),
            Self::TimedOut => f.write_str("The provider did not answer in time."),
            Self::Cancelled => f.write_str("Stopped."),
            Self::TooLarge => f.write_str("The answer was larger than the app accepts, so it was cut off."),
            Self::BadAnswer(m) => write!(f, "The provider's answer could not be read: {m}"),
            Self::NoKey => f.write_str("This provider needs a key. Add one in Settings, Providers."),
            Self::NoConsent => f.write_str("You have not allowed this provider to receive your documents' text yet. Settings, Providers shows exactly what would be sent."),
            Self::Offline => f.write_str("The agent is switched off (offline). No model is called."),
            Self::CapReached(m) => write!(f, "{m}"),
            Self::BadSetup(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AgentError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_must_be_https_or_this_computer() {
        assert!(Endpoint::parse(Kind::Openai, "https://api.openai.com/v1", "gpt-5").is_ok());
        assert!(Endpoint::parse(Kind::Ollama, "http://127.0.0.1:11434", "llama3.1").is_ok());
        assert_eq!(
            Endpoint::parse(Kind::Compat, "http://example.org/v1", "m").unwrap_err(),
            EndpointError::Insecure
        );
        assert_eq!(
            Endpoint::parse(Kind::Compat, "https://user:pw@example.org", "m").unwrap_err(),
            EndpointError::Credentials
        );
        assert_eq!(
            Endpoint::parse(Kind::Compat, "https://example.org/v1?key=abc", "m").unwrap_err(),
            EndpointError::Credentials
        );
        assert_eq!(
            Endpoint::parse(Kind::Compat, "ftp://example.org", "m").unwrap_err(),
            EndpointError::NotAnAddress
        );
        assert_eq!(
            Endpoint::parse(Kind::Compat, "https://example.org", "a b").unwrap_err(),
            EndpointError::BadModel
        );
        assert_eq!(
            Endpoint::parse(Kind::Ollama, "http://127.0.0.1:11434", "m")
                .unwrap()
                .host(),
            "127.0.0.1:11434"
        );
    }

    #[test]
    fn a_key_shows_four_characters_only_when_it_is_long() {
        assert!(ApiKey::new("short").is_none());
        assert!(ApiKey::new("has a space inside the key").is_none());
        assert_eq!(ApiKey::new("sk-0123456789abcdef").unwrap().hint(), "cdef");
        assert_eq!(ApiKey::new("abcdefgh").unwrap().hint(), "····");
    }

    #[test]
    fn tool_names_survive_the_wire() {
        assert_eq!(wire_name("history.read"), "history__read");
        assert_eq!(from_wire_name("history__read"), "history.read");
        assert_eq!(
            from_wire_name(&wire_name("review.propose_comment")),
            "review.propose_comment"
        );
    }
}
