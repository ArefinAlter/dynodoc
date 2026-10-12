//! Paseto v4 local access tokens (docs/17, NFR-8).
//!
//! Tokens use authenticated encryption; active-account and revocation checks additionally
//! consult the database on each authenticated request. We use Paseto v4 local (symmetric
//! AEAD) rather than JWT — one algorithm per version, no `alg` field to confuse, no
//! `none` bypass. Claims bind identity, account generation and an optional login ID;
//! the per-document role is
//! resolved separately at request time, never embedded in a long-lived token.

use chrono::{SecondsFormat, Utc};
use pasetors::claims::{Claims, ClaimsValidationRules};
use pasetors::footer::Footer;
use pasetors::keys::SymmetricKey;
use pasetors::token::UntrustedToken;
use pasetors::version4::V4;
use pasetors::{local, Local};
use uuid::Uuid;

use super::AuthError;

/// Mint a Paseto v4 local token for `identity_id`, valid for `ttl_seconds` from now.
/// A non-positive `ttl_seconds` produces an already-expired token (used in tests).
pub fn mint(key: &[u8], identity_id: Uuid, ttl_seconds: i64) -> Result<String, AuthError> {
    mint_generation(key, identity_id, ttl_seconds, 0)
}
pub fn mint_generation(
    key: &[u8],
    identity_id: Uuid,
    ttl_seconds: i64,
    generation: i64,
) -> Result<String, AuthError> {
    mint_bound_session(key, identity_id, ttl_seconds, generation, None)
}

pub fn mint_bound_session(
    key: &[u8],
    identity_id: Uuid,
    ttl_seconds: i64,
    generation: i64,
    session_id: Option<Uuid>,
) -> Result<String, AuthError> {
    let sk = SymmetricKey::<V4>::from(key).map_err(|_| AuthError::InvalidKey)?;

    let expires = Utc::now() + chrono::Duration::seconds(ttl_seconds);
    let mut claims = Claims::new().map_err(|e| AuthError::TokenMint(e.to_string()))?;
    claims
        .expiration(&expires.to_rfc3339_opts(SecondsFormat::Secs, true))
        .map_err(|e| AuthError::TokenMint(e.to_string()))?;
    claims
        .add_additional("identity_id", identity_id.to_string())
        .map_err(|e| AuthError::TokenMint(e.to_string()))?;

    claims
        .add_additional("session_generation", generation)
        .map_err(|e| AuthError::TokenMint(e.to_string()))?;
    if let Some(id) = session_id {
        claims
            .add_additional("session_id", id.to_string())
            .map_err(|e| AuthError::TokenMint(e.to_string()))?;
    }
    local::encrypt(&sk, &claims, None, None).map_err(|e| AuthError::TokenMint(e.to_string()))
}

/// Verify a Paseto v4 local token and return its `identity_id`. Rejects an expired,
/// tampered, or malformed token, or one signed with a different key. Expiry is checked
/// against the real clock by the default [`ClaimsValidationRules`].
pub fn verify(key: &[u8], token: &str) -> Result<Uuid, AuthError> {
    verify_session(key, token).map(|(identity, _)| identity)
}
pub fn verify_session(key: &[u8], token: &str) -> Result<(Uuid, i64), AuthError> {
    verify_bound_session(key, token).map(|(id, generation, _)| (id, generation))
}

pub fn verify_bound_session(
    key: &[u8],
    token: &str,
) -> Result<(Uuid, i64, Option<Uuid>), AuthError> {
    let sk = SymmetricKey::<V4>::from(key).map_err(|_| AuthError::InvalidKey)?;

    let untrusted =
        UntrustedToken::<Local, V4>::try_from(token).map_err(|_| AuthError::TokenInvalid)?;
    let rules = ClaimsValidationRules::new();
    let trusted =
        local::decrypt(&sk, &untrusted, &rules, None, None).map_err(|_| AuthError::TokenInvalid)?;

    let claims = trusted.payload_claims().ok_or(AuthError::TokenInvalid)?;
    let id = claims
        .get_claim("identity_id")
        .and_then(|v| v.as_str())
        .ok_or(AuthError::TokenInvalid)?;
    let generation = match claims.get_claim("session_generation") {
        Some(value) => value.as_i64().ok_or(AuthError::TokenInvalid)?,
        None => 0,
    };
    let session_id = claims
        .get_claim("session_id")
        .map(|value| {
            value
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or(AuthError::TokenInvalid)
        })
        .transpose()?;
    Ok((
        Uuid::parse_str(id).map_err(|_| AuthError::TokenInvalid)?,
        generation,
        session_id,
    ))
}

// --- project-scoped device sessions -----------------------------------------------
//
// A different token *type*, not a different claim in an account token. Paseto v4
// authenticates the implicit assertion inside the tag, and every account validator
// above decrypts with no implicit assertion, so it rejects these tokens before it
// reads a claim. The footer and claim names are belt and braces: a footer alone
// would not separate the types (the library only compares a footer when asked).

const DEVICE_TYPE: &str = "dynodoc.project-device-session.v1";
/// The footer is the JSON object `{"typ":"dynodoc.project-device-session.v1"}`.
const DEVICE_FOOTER_JSON: &[u8] = br#"{"typ":"dynodoc.project-device-session.v1"}"#;
fn device_footer() -> Footer {
    let mut footer = Footer::new();
    footer
        .add_additional("typ", DEVICE_TYPE)
        .expect("static footer claim");
    footer
}
const DEVICE_IMPLICIT: &[u8] = b"dynodoc.project-device-session.v1/implicit";
/// Lifetime of a project device token; also pinned in the signed challenge.
pub const DEVICE_SESSION_TTL_SECONDS: i64 =
    engine_core::repository::device_session::TOKEN_TTL_SECONDS;

/// What a verified project device token asserts. Authority is still re-read from
/// the database on every request; none of this is trusted beyond identification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceClaims {
    pub identity: Uuid,
    pub key: Uuid,
    pub project: Uuid,
    pub session: Uuid,
    pub scopes: Vec<String>,
    pub generation: i64,
}

/// Mint a project device token. A non-positive `ttl_seconds` yields an already
/// expired token (tests only).
pub fn mint_project_device(
    key: &[u8],
    claims: &DeviceClaims,
    ttl_seconds: i64,
) -> Result<String, AuthError> {
    let sk = SymmetricKey::<V4>::from(key).map_err(|_| AuthError::InvalidKey)?;
    let fail = |e: pasetors::errors::Error| AuthError::TokenMint(e.to_string());
    let mut out = Claims::new().map_err(fail)?;
    let expires = Utc::now() + chrono::Duration::seconds(ttl_seconds);
    out.expiration(&expires.to_rfc3339_opts(SecondsFormat::Secs, true))
        .map_err(fail)?;
    out.token_identifier(&claims.session.to_string())
        .map_err(fail)?;
    for (name, value) in [
        ("typ", DEVICE_TYPE.to_owned()),
        ("device_identity", claims.identity.to_string()),
        ("device_key", claims.key.to_string()),
        ("project", claims.project.to_string()),
    ] {
        out.add_additional(name, value).map_err(fail)?;
    }
    out.add_additional("scopes", claims.scopes.clone())
        .map_err(fail)?;
    out.add_additional("device_generation", claims.generation)
        .map_err(fail)?;
    local::encrypt(&sk, &out, Some(&device_footer()), Some(DEVICE_IMPLICIT)).map_err(fail)
}

/// True when the token carries the device footer. Used only to choose which
/// verifier runs; the verifier itself is what authenticates.
pub fn is_project_device_token(token: &str) -> bool {
    UntrustedToken::<Local, V4>::try_from(token)
        .map(|t| t.untrusted_footer() == DEVICE_FOOTER_JSON)
        .unwrap_or(false)
}

/// Verify a project device token. Account tokens, connector keys and tokens from
/// another key fail here exactly as device tokens fail the account verifier.
pub fn verify_project_device(key: &[u8], token: &str) -> Result<DeviceClaims, AuthError> {
    let sk = SymmetricKey::<V4>::from(key).map_err(|_| AuthError::InvalidKey)?;
    let untrusted =
        UntrustedToken::<Local, V4>::try_from(token).map_err(|_| AuthError::TokenInvalid)?;
    let trusted = local::decrypt(
        &sk,
        &untrusted,
        &ClaimsValidationRules::new(),
        Some(&device_footer()),
        Some(DEVICE_IMPLICIT),
    )
    .map_err(|_| AuthError::TokenInvalid)?;
    let claims = trusted.payload_claims().ok_or(AuthError::TokenInvalid)?;
    let text = |name: &str| {
        claims
            .get_claim(name)
            .and_then(|v| v.as_str())
            .ok_or(AuthError::TokenInvalid)
    };
    let uuid = |name: &str| Uuid::parse_str(text(name)?).map_err(|_| AuthError::TokenInvalid);
    if text("typ")? != DEVICE_TYPE {
        return Err(AuthError::TokenInvalid);
    }
    let instant = |name: &str| {
        chrono::DateTime::parse_from_rfc3339(text(name)?).map_err(|_| AuthError::TokenInvalid)
    };
    // The service only issues one-hour tokens; a longer lifetime is a forgery or bug.
    if (instant("exp")? - instant("iat")?).num_seconds() > DEVICE_SESSION_TTL_SECONDS + 5 {
        return Err(AuthError::TokenInvalid);
    }
    let scopes: Vec<String> = claims
        .get_claim("scopes")
        .and_then(|v| v.as_array())
        .ok_or(AuthError::TokenInvalid)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or(AuthError::TokenInvalid))
        .collect::<Result<_, _>>()?;
    // Known, sorted, unique, non-empty: exactly what the service issues.
    match engine_core::repository::device_session::normalize_scopes(&scopes) {
        Ok(normalized) if normalized == scopes => {}
        _ => return Err(AuthError::TokenInvalid),
    }
    Ok(DeviceClaims {
        identity: uuid("device_identity")?,
        key: uuid("device_key")?,
        project: uuid("project")?,
        session: uuid("jti")?,
        scopes,
        generation: claims
            .get_claim("device_generation")
            .and_then(|v| v.as_i64())
            .ok_or(AuthError::TokenInvalid)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fixed 32-byte test key (never used outside tests).
    const KEY: [u8; 32] = [7u8; 32];

    fn device() -> DeviceClaims {
        DeviceClaims {
            identity: Uuid::from_u128(1),
            key: Uuid::from_u128(2),
            project: Uuid::from_u128(3),
            session: Uuid::from_u128(4),
            scopes: vec!["history:read".into(), "history:write".into()],
            generation: 5,
        }
    }

    #[test]
    fn device_and_account_tokens_are_not_interchangeable() {
        let claims = device();
        let device_token = mint_project_device(&KEY, &claims, 3600).unwrap();
        assert_eq!(verify_project_device(&KEY, &device_token).unwrap(), claims);
        assert!(is_project_device_token(&device_token));
        // Every account verifier fails closed on the new token type.
        assert!(verify(&KEY, &device_token).is_err());
        assert!(verify_session(&KEY, &device_token).is_err());
        assert!(verify_bound_session(&KEY, &device_token).is_err());
        // And no account token can become a device token, even for the same key.
        let account =
            mint_bound_session(&KEY, claims.identity, 3600, 5, Some(claims.session)).unwrap();
        assert!(!is_project_device_token(&account));
        assert!(verify_project_device(&KEY, &account).is_err());
        assert!(verify_project_device(&[9u8; 32], &device_token).is_err());
    }

    #[test]
    fn implicit_assertion_alone_separates_the_cryptographic_layer() {
        // The account validator's exact decrypt call fails for a device token
        // *before* any claim is read, and the reverse. Claim names are not what
        // protects the boundary.
        let sk = SymmetricKey::<V4>::from(&KEY).unwrap();
        let rules = ClaimsValidationRules::new();
        let device_token = mint_project_device(&KEY, &device(), 3600).unwrap();
        let untrusted = UntrustedToken::<Local, V4>::try_from(device_token.as_str()).unwrap();
        assert!(local::decrypt(&sk, &untrusted, &rules, None, None).is_err());
        assert!(local::decrypt(&sk, &untrusted, &rules, Some(&device_footer()), None).is_err());
        assert!(local::decrypt(
            &sk,
            &untrusted,
            &rules,
            Some(&device_footer()),
            Some(DEVICE_IMPLICIT)
        )
        .is_ok());
        let account = mint(&KEY, Uuid::from_u128(1), 3600).unwrap();
        let untrusted = UntrustedToken::<Local, V4>::try_from(account.as_str()).unwrap();
        assert!(local::decrypt(&sk, &untrusted, &rules, None, None).is_ok());
        assert!(local::decrypt(
            &sk,
            &untrusted,
            &rules,
            Some(&device_footer()),
            Some(DEVICE_IMPLICIT)
        )
        .is_err());
        assert!(local::decrypt(&sk, &untrusted, &rules, None, Some(DEVICE_IMPLICIT)).is_err());
    }

    #[test]
    fn device_token_rejects_expiry_tampering_and_missing_separators() {
        let claims = device();
        assert!(
            verify_project_device(&KEY, &mint_project_device(&KEY, &claims, -10).unwrap()).is_err()
        );
        let token = mint_project_device(&KEY, &claims, 3600).unwrap();
        assert!(verify_project_device(&KEY, &format!("{token}x")).is_err());
        // A token with this type's claims and footer but no implicit assertion (what
        // a different encryptor, or a library with the assertion dropped, would
        // produce) is refused: the verifier requires the assertion.
        let sk = SymmetricKey::<V4>::from(&KEY).unwrap();
        let mut forged = Claims::new().unwrap();
        for (name, value) in [
            ("typ", DEVICE_TYPE.to_owned()),
            ("device_identity", claims.identity.to_string()),
            ("device_key", claims.key.to_string()),
            ("project", claims.project.to_string()),
        ] {
            forged.add_additional(name, value).unwrap();
        }
        forged
            .add_additional("scopes", vec!["history:read"])
            .unwrap();
        forged.add_additional("device_generation", 5).unwrap();
        forged
            .token_identifier(&claims.session.to_string())
            .unwrap();
        let no_assertion = local::encrypt(&sk, &forged, Some(&device_footer()), None).unwrap();
        assert!(verify_project_device(&KEY, &no_assertion).is_err());
        let no_footer = local::encrypt(&sk, &forged, None, Some(DEVICE_IMPLICIT)).unwrap();
        assert!(verify_project_device(&KEY, &no_footer).is_err());
        // Correct envelope but unknown, unsorted or empty scopes, or a wrong type.
        for scopes in [vec!["admin"], vec!["history:write", "history:read"], vec![]] {
            let mut claims = forged.clone();
            claims.add_additional("scopes", scopes).unwrap();
            let token = local::encrypt(&sk, &claims, Some(&device_footer()), Some(DEVICE_IMPLICIT))
                .unwrap();
            assert!(verify_project_device(&KEY, &token).is_err());
        }
        let mut wrong = forged.clone();
        wrong.add_additional("typ", "account").unwrap();
        let token =
            local::encrypt(&sk, &wrong, Some(&device_footer()), Some(DEVICE_IMPLICIT)).unwrap();
        assert!(verify_project_device(&KEY, &token).is_err());
        // A day-long token (forged lifetime) is refused even when well formed.
        let mut long = forged.clone();
        long.expiration(
            &(Utc::now() + chrono::Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true),
        )
        .unwrap();
        let token =
            local::encrypt(&sk, &long, Some(&device_footer()), Some(DEVICE_IMPLICIT)).unwrap();
        assert!(verify_project_device(&KEY, &token).is_err());
        // The same forged claims, correctly enveloped with a one-hour life, pass.
        let ok =
            local::encrypt(&sk, &forged, Some(&device_footer()), Some(DEVICE_IMPLICIT)).unwrap();
        assert!(verify_project_device(&KEY, &ok).is_ok());
    }

    #[test]
    fn mint_then_verify_round_trips_identity() {
        let id = Uuid::from_u128(0x1234);
        let token = mint(&KEY, id, 3600).unwrap();
        assert_eq!(verify(&KEY, &token).unwrap(), id);
    }

    #[test]
    fn expired_token_is_rejected() {
        let id = Uuid::from_u128(1);
        let token = mint(&KEY, id, -10).unwrap(); // already expired
        assert!(matches!(verify(&KEY, &token), Err(AuthError::TokenInvalid)));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let id = Uuid::from_u128(1);
        let token = mint(&KEY, id, 3600).unwrap();
        let other = [9u8; 32];
        assert!(matches!(
            verify(&other, &token),
            Err(AuthError::TokenInvalid)
        ));
    }

    #[test]
    fn tampered_token_is_rejected() {
        let id = Uuid::from_u128(1);
        let mut token = mint(&KEY, id, 3600).unwrap();
        // Flip a character in the payload section.
        token.push('x');
        assert!(matches!(verify(&KEY, &token), Err(AuthError::TokenInvalid)));
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert!(matches!(
            verify(&KEY, "not-a-paseto-token"),
            Err(AuthError::TokenInvalid)
        ));
    }

    #[test]
    fn short_key_is_invalid() {
        assert!(matches!(
            mint(&[0u8; 8], Uuid::nil(), 60),
            Err(AuthError::InvalidKey)
        ));
    }
}
