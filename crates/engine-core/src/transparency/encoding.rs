//! Lowercase hex and strict standard base64 (RFC 4648 section 4: padded, with the
//! canonical encoding of the final group). Strictness matters: a verifier that
//! accepts a second spelling of the same bytes lets a note or proof be re-encoded
//! without breaking its signature.
use super::Error;

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 15)] as char);
    }
    out
}

pub(crate) fn unhex(text: &str) -> Result<Vec<u8>, Error> {
    fn digit(c: u8) -> Result<u8, Error> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err(Error::Hex("only lowercase hexadecimal digits are allowed")),
        }
    }
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(Error::Hex("odd length"));
    }
    bytes
        .chunks(2)
        .map(|pair| Ok(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

pub(crate) fn unhex_array<const N: usize>(text: &str) -> Result<[u8; N], Error> {
    unhex(text)?
        .try_into()
        .map_err(|_| Error::Hex("unexpected length"))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn b64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn unb64(text: &str) -> Result<Vec<u8>, Error> {
    fn sextet(c: u8) -> Result<u32, Error> {
        match c {
            b'A'..=b'Z' => Ok(u32::from(c - b'A')),
            b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(Error::Base64("character outside the standard alphabet")),
        }
    }
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Error::Base64("length is not a multiple of four"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, quad) in bytes.chunks(4).enumerate() {
        let last = (i + 1) * 4 == bytes.len();
        let pad = quad.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return Err(Error::Base64("misplaced padding"));
        }
        let mut n = 0u32;
        for c in &quad[..4 - pad] {
            n = n << 6 | sextet(*c)?;
        }
        n <<= 6 * pad;
        // The unused low bits of the final group must be zero (canonical form).
        if n & ((1 << (8 * pad)) - 1) != 0 {
            return Err(Error::Base64("non-canonical final group"));
        }
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pad]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (raw, text) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(b64(raw), text);
            assert_eq!(unb64(text).unwrap(), raw);
        }
    }

    #[test]
    fn base64_rejects_noncanonical_and_malformed_input() {
        for bad in [
            "Zh==", "Zm9=", "Zg=", "Zg", "=Zg=", "Zg==Zg==", "Zm9v\n", "Zm 9", "Zm9v====", "Zm-v",
            "Zm_v",
        ] {
            assert!(unb64(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn hex_is_lowercase_only() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(unhex("00abff").unwrap(), [0x00, 0xab, 0xff]);
        assert!(unhex("00ABFF").is_err());
        assert!(unhex("0").is_err());
        assert!(unhex("zz").is_err());
        assert!(unhex_array::<2>("00abff").is_err());
    }
}
