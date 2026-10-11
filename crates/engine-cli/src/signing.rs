//! Unencrypted DER Ed25519 PKCS#8 imports for both local signing commands.
use ring::signature::Ed25519KeyPair;
use std::{io::Read, path::Path};

fn parse(bytes: &[u8]) -> anyhow::Result<Ed25519KeyPair> {
    anyhow::ensure!(bytes.len() <= 4096, "signing key file too large");
    // OpenSSL exports v1 without a redundant public key. Ring
    // derives that key from the seed; v2 still checks both components match.
    Ed25519KeyPair::from_pkcs8_maybe_unchecked(bytes)
        .map_err(|_| anyhow::anyhow!("invalid unencrypted DER Ed25519 PKCS#8 key"))
}
pub(crate) fn load(path: &Path) -> anyhow::Result<Ed25519KeyPair> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::repository::attestation::SignedReceipt;
    #[test]
    fn standard_v1_and_v2_keys_work_without_weakening_component_checks() {
        // Public synthetic seed, matching the independent OpenSSL signature fixture.
        let mut v1 = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        v1.extend([7; 32]);
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../engine-core/tests/fixtures/ref-receipt-signatures-v1.json"
        ))
        .unwrap();
        let expected: SignedReceipt =
            serde_json::from_value(fixture["first"]["signed"].clone()).unwrap();
        assert_eq!(
            SignedReceipt::sign(expected.receipt.clone(), &parse(&v1).unwrap()).unwrap(),
            expected
        );
        assert!(parse(&v1[..v1.len() - 1]).is_err());
        v1[11] = 0x71; // Ed448 is not an Ed25519 key.
        assert!(parse(&v1).is_err());
        assert!(parse(&vec![0; 4097]).is_err());
        let v2 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        assert!(parse(v2.as_ref()).is_ok());
        let mut mismatch = v2.as_ref().to_vec();
        *mismatch.last_mut().unwrap() ^= 1;
        assert!(
            parse(&mismatch).is_err(),
            "v2 public/private consistency must remain enforced"
        );
    }
}
