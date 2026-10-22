//! Just enough DER to read what an operator wants from a certificate: names, validity, subject
//! alternative names. It trusts nothing it reads (certificate trust is rustls's job); a
//! certificate this reader cannot parse is reported as unreadable, never as valid.

use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CertInfo {
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub dns_names: Vec<String>,
    #[serde(skip)]
    pub not_after_time: DateTime<Utc>,
}

impl CertInfo {
    /// Whole days from `now` until the certificate expires (negative once expired).
    pub fn days_left(&self, now: DateTime<Utc>) -> i64 {
        (self.not_after_time - now).num_days()
    }
    /// The evaluation certificate of the bundle says so in its subject.
    pub fn is_evaluation(&self) -> bool {
        self.subject.contains("EVALUATION")
    }
}

/// One DER element: tag, content, rest of the input.
fn tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *input.first()?;
    let first = *input.get(1)?;
    let (len, header) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 || input.len() < 2 + n {
            return None;
        }
        let mut len = 0usize;
        for b in &input[2..2 + n] {
            len = (len << 8) | *b as usize;
        }
        (len, 2 + n)
    };
    let end = header.checked_add(len)?;
    if input.len() < end {
        return None;
    }
    Some((tag, &input[header..end], &input[end..]))
}

fn children(mut content: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    while !content.is_empty() {
        match tlv(content) {
            Some((tag, body, rest)) => {
                out.push((tag, body));
                content = rest;
            }
            None => break,
        }
    }
    out
}

fn parse_time(tag: u8, body: &[u8]) -> Option<DateTime<Utc>> {
    let text = std::str::from_utf8(body).ok()?;
    let digits = text.strip_suffix('Z')?;
    let (year, rest) = match tag {
        0x17 => {
            let yy: i32 = digits.get(0..2)?.parse().ok()?;
            (
                if yy >= 50 { 1900 + yy } else { 2000 + yy },
                digits.get(2..)?,
            )
        }
        0x18 => (digits.get(0..4)?.parse().ok()?, digits.get(4..)?),
        _ => return None,
    };
    let field = |a: usize| -> Option<u32> { rest.get(a..a + 2)?.parse().ok() };
    let date = NaiveDate::from_ymd_opt(year, field(0)?, field(2)?)?;
    Some(
        date.and_hms_opt(field(4)?, field(6)?, field(8).unwrap_or(0))?
            .and_utc(),
    )
}

fn name_to_string(name: &[u8]) -> String {
    let mut parts = Vec::new();
    for (_, rdn) in children(name) {
        for (_, atv) in children(rdn) {
            let items = children(atv);
            if let (Some((0x06, oid)), Some((_, value))) = (items.first(), items.get(1)) {
                let label = match oid {
                    [0x55, 0x04, 0x03] => "CN",
                    [0x55, 0x04, 0x0a] => "O",
                    [0x55, 0x04, 0x0b] => "OU",
                    [0x55, 0x04, 0x06] => "C",
                    [0x55, 0x04, 0x07] => "L",
                    [0x55, 0x04, 0x08] => "ST",
                    _ => "?",
                };
                parts.push(format!("{label}={}", String::from_utf8_lossy(value)));
            }
        }
    }
    parts.join(", ")
}

/// Read a DER certificate. None when it is not one this reader understands.
pub fn parse_cert(der: &[u8]) -> Option<CertInfo> {
    let (0x30, cert, _) = tlv(der)? else {
        return None;
    };
    let (0x30, tbs, _) = tlv(cert)? else {
        return None;
    };
    let mut fields = children(tbs).into_iter().peekable();
    if matches!(fields.peek(), Some((0xa0, _))) {
        fields.next(); // version
    }
    let _serial = fields.next()?;
    let _signature = fields.next()?;
    let (_, issuer) = fields.next()?;
    let (_, validity) = fields.next()?;
    let (_, subject) = fields.next()?;
    let _spki = fields.next()?;
    let times = children(validity);
    let (nb_tag, nb) = *times.first()?;
    let (na_tag, na) = *times.get(1)?;
    let not_before = parse_time(nb_tag, nb)?;
    let not_after = parse_time(na_tag, na)?;
    let mut dns_names = Vec::new();
    for (tag, body) in fields {
        if tag != 0xa3 {
            continue;
        }
        let Some((0x30, exts, _)) = tlv(body) else {
            continue;
        };
        for (_, ext) in children(exts) {
            let parts = children(ext);
            if parts.first().map(|(_, oid)| *oid) == Some(&[0x55, 0x1d, 0x11][..]) {
                if let Some((0x04, octets)) = parts.last() {
                    if let Some((0x30, names, _)) = tlv(octets) {
                        for (tag, value) in children(names) {
                            if tag == 0x82 {
                                dns_names.push(String::from_utf8_lossy(value).into_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    Some(CertInfo {
        subject: name_to_string(subject),
        issuer: name_to_string(issuer),
        not_before: crate::util::iso(not_before),
        not_after: crate::util::iso(not_after),
        dns_names,
        not_after_time: not_after,
    })
}

/// The certificates in a PEM text, as DER.
pub fn pem_certificates(text: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "-----BEGIN CERTIFICATE-----" {
            current = Some(String::new());
        } else if line == "-----END CERTIFICATE-----" {
            if let Some(b64) = current.take() {
                if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()) {
                    out.push(der);
                }
            }
        } else if let Some(b64) = current.as_mut() {
            b64.push_str(line);
        }
    }
    out
}

#[cfg(test)]
pub const TEST_CERT_B64: &str = "MIICDDCCAbKgAwIBAgIUFgwDgfJIEKHGABlENRAmyjIH/+4wCgYIKoZIzj0EAwIwRDEQMA4GA1UECgwHRHlub2RvYzEwMC4GA1UEAwwnRHlub2RvYyBFVkFMVUFUSU9OIC0gTk9UIEZPUiBQUk9EVUNUSU9OMB4XDTI2MTAyMjE0NDQ1NFoXDTQ2MTAxNzE0NDQ1NFowRDEQMA4GA1UECgwHRHlub2RvYzEwMC4GA1UEAwwnRHlub2RvYyBFVkFMVUFUSU9OIC0gTk9UIEZPUiBQUk9EVUNUSU9OMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEbZxygtu+ZUbgPhvTkNWR2x7fX4IgTfzsXWO4vFIl52yTWD+FIQzFa8AQZ+Tgb8HlAIz7x/0cCmgh77uAh+CyLaOBgTB/MB0GA1UdDgQWBBS31zycZUr01lcaCxji4fkSv/+xYTAfBgNVHSMEGDAWgBS31zycZUr01lcaCxji4fkSv/+xYTAPBgNVHRMBAf8EBTADAQH/MCwGA1UdEQQlMCOCEGRvY3MuZXhhbXBsZS5vcmeCCWxvY2FsaG9zdIcEfwAAATAKBggqhkjOPQQDAgNIADBFAiEAt5IWT4tU5WmkKF8mM+qC+auQHfcReRY+CgBSHnAxH7gCIDVz7y+Mn26JpBM3XdefMWGYj8RAPeOU9z058XNNca+l";

/// A second fixture: an ordinary certificate (not the evaluation label), valid until 2046-10-17T15:04:07Z.
#[cfg(test)]
pub const TEST_CERT2_B64: &str = "MIIB1jCCAXygAwIBAgIUQwkfgpzrkQ5G9CTPwNz8KbcGJukwCgYIKoZIzj0EAwIwMjEVMBMGA1UECgwMRXhhbXBsZSBMdGQuMRkwFwYDVQQDDBBkb2NzLmV4YW1wbGUub3JnMB4XDTI2MTAyMjE1MDQwN1oXDTQ2MTAxNzE1MDQwN1owMjEVMBMGA1UECgwMRXhhbXBsZSBMdGQuMRkwFwYDVQQDDBBkb2NzLmV4YW1wbGUub3JnMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEmS5gzN6DX9CShxa+SbepIBJ9TLYe35FfHVwjX6vhgJhGEX3pXGzhk2/GcN1CyFl2kf8oWY9z7Ac4FGADepKCO6NwMG4wHQYDVR0OBBYEFKCCw8rJElVHajte5njF1Wyy6ngtMB8GA1UdIwQYMBaAFKCCw8rJElVHajte5njF1Wyy6ngtMA8GA1UdEwEB/wQFMAMBAf8wGwYDVR0RBBQwEoIQZG9jcy5leGFtcGxlLm9yZzAKBggqhkjOPQQDAgNIADBFAiBU/upvbisymSz0KVUJN/Wdpz8KAiVUukjoRBasSvMbUwIhAK/0dE4udEdDtcESQ5OJZq0y/US/HZ/jtTfPV1h160pD";

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn der() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(TEST_CERT_B64)
            .unwrap()
    }

    #[test]
    fn reads_names_validity_and_alternative_names() {
        let info = parse_cert(&der()).expect("parses");
        assert_eq!(
            info.subject,
            "O=Dynodoc, CN=Dynodoc EVALUATION - NOT FOR PRODUCTION"
        );
        assert_eq!(info.issuer, info.subject);
        assert_eq!(info.not_before, "2026-10-22T14:44:54Z");
        assert_eq!(info.not_after, "2046-10-17T14:44:54Z");
        assert_eq!(info.dns_names, vec!["docs.example.org", "localhost"]);
        assert!(info.is_evaluation());
    }

    #[test]
    fn counts_days_left_and_goes_negative_after_expiry() {
        let info = parse_cert(&der()).unwrap();
        let before = Utc.with_ymd_and_hms(2046, 10, 7, 14, 44, 54).unwrap();
        let after = Utc.with_ymd_and_hms(2046, 10, 20, 14, 44, 54).unwrap();
        assert_eq!(info.days_left(before), 10);
        assert!(info.days_left(after) < 0);
    }

    #[test]
    fn garbage_and_truncation_are_not_certificates() {
        assert!(parse_cert(b"").is_none());
        assert!(parse_cert(b"not a certificate").is_none());
        let full = der();
        assert!(parse_cert(&full[..full.len() / 2]).is_none());
    }

    #[test]
    fn pem_blocks_are_decoded_and_other_blocks_ignored() {
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            TEST_CERT_B64
        );
        let found = pem_certificates(&pem);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], der());
    }

    #[test]
    fn two_digit_years_follow_rfc_5280() {
        assert_eq!(
            parse_time(0x17, b"491231235959Z")
                .unwrap()
                .format("%Y")
                .to_string(),
            "2049"
        );
        assert_eq!(
            parse_time(0x17, b"500101000000Z")
                .unwrap()
                .format("%Y")
                .to_string(),
            "1950"
        );
        assert!(parse_time(0x17, b"garbage").is_none());
    }
}
