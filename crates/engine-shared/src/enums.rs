//! Domain vocabulary enums mirroring the schema's `CHECK`-constrained text columns.
//!
//! These are stored as plain `text` in Postgres (the `CHECK` constraint is the
//! self-documenting source of truth), so the `FromRow` row structs carry the raw
//! `String`. These enums are the typed, app-facing view: build events and validate
//! node/document state against them. `as_str()` returns the exact stored spelling.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Error returned when a stored string does not match any known variant.
#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} value: {value:?}")]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $repr:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        // serde uses the stored string repr (`as_str()` / `FromStr`), not the variant
        // name, so the wire form, the JSONB payload, and the Postgres `CHECK` columns
        // all agree (e.g. `NodeType::Form` ⇄ `"form"`, never `"Form"`).
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                raw.parse().map_err(serde::de::Error::custom)
            }
        }

        impl $name {
            /// The exact string stored in Postgres for this variant.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $repr),+
                }
            }

            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
        }

        impl FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($repr => Ok(Self::$variant),)+
                    other => Err(ParseEnumError { kind: $kind, value: other.to_string() }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum!(
    /// Typed node kinds. Form/Section/Item/Choice are the questionnaire tree
    /// (docs/04 §D.1); Clause/Paragraph are the policy-document kinds the engine
    /// stays compatible with for the later consultation regime.
    NodeType, "node type", {
        Form => "form",
        Section => "section",
        Item => "item",
        Choice => "choice",
        Clause => "clause",
        Paragraph => "paragraph",
    }
);

string_enum!(
    /// Document lifecycle. `Deployed` is reached via the Deployed event, which pins
    /// an immutable snapshot.
    DocumentStatus, "document status", {
        Draft => "draft",
        Deployed => "deployed",
        Archived => "archived",
    }
);

string_enum!(
    /// The semantic event vocabulary for the Research IDE PoC. Mirrors the `CHECK`
    /// on `event.type` (0002_events.sql).
    EventType, "event type", {
        NodeCreated => "NodeCreated",
        NodeMoved => "NodeMoved",
        NodeDeleted => "NodeDeleted",
        NodeRestored => "NodeRestored",
        FieldEdited => "FieldEdited",
        RichTextPatched => "RichTextPatched",
        ChoiceAdded => "ChoiceAdded",
        ChoiceRemoved => "ChoiceRemoved",
        CommentAdded => "CommentAdded",
        CommentReplied => "CommentReplied",
        CommentResolved => "CommentResolved",
        SuggestionProposed => "SuggestionProposed",
        SuggestionAccepted => "SuggestionAccepted",
        SuggestionRejected => "SuggestionRejected",
        Deployed => "Deployed",
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_str() {
        for &t in NodeType::ALL {
            assert_eq!(t.as_str().parse::<NodeType>().unwrap(), t);
        }
        for &s in DocumentStatus::ALL {
            assert_eq!(s.as_str().parse::<DocumentStatus>().unwrap(), s);
        }
        for &e in EventType::ALL {
            assert_eq!(e.as_str().parse::<EventType>().unwrap(), e);
        }
    }

    #[test]
    fn unknown_value_errors() {
        assert!("nope".parse::<NodeType>().is_err());
    }

    #[test]
    fn serde_uses_the_stored_string_repr_not_the_variant_name() {
        // Wire/JSONB form is the lowercase `as_str()` repr, so it matches the SQL
        // `CHECK` columns and what the API accepts (regression: was `"Form"`).
        assert_eq!(serde_json::to_value(NodeType::Form).unwrap(), "form");
        assert_eq!(
            serde_json::to_value(DocumentStatus::Draft).unwrap(),
            "draft"
        );
        let back: NodeType = serde_json::from_value(serde_json::json!("form")).unwrap();
        assert_eq!(back, NodeType::Form);
        assert!(serde_json::from_value::<NodeType>(serde_json::json!("Form")).is_err());
    }
}
