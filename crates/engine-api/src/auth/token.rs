//! Paseto v4 local access tokens (docs/17, NFR-8).
//!
//! Tokens use authenticated encryption; active-account and revocation checks additionally
//! consult the database on each authenticated request. We use Paseto v4 local (symmetric
//! AEAD) rather than JWT — one algorithm per version, no `alg` field to confuse, no
//! `none` bypass. The only claim we carry is `identity_id`; the per-document role is
//! resolved separately at request time, never embedded in a long-lived token.

use chrono::{SecondsFormat, Utc};
use pasetors::claims::{Claims, ClaimsValidationRules};
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
    local::encrypt(&sk, &claims, None, None).map_err(|e| AuthError::TokenMint(e.to_string()))
}

/// Verify a Paseto v4 local token and return its `identity_id`. Rejects an expired,
/// tampered, or malformed token, or one signed with a different key. Expiry is checked
/// against the real clock by the default [`ClaimsValidationRules`].
pub fn verify(key: &[u8], token: &str) -> Result<Uuid, AuthError> {
    verify_session(key, token).map(|(identity, _)| identity)
}
pub fn verify_session(key: &[u8], token: &str) -> Result<(Uuid, i64), AuthError> {
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
    Ok((
        Uuid::parse_str(id).map_err(|_| AuthError::TokenInvalid)?,
        generation,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fixed 32-byte test key (never used outside tests).
    const KEY: [u8; 32] = [7u8; 32];

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
