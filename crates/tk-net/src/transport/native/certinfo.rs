// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! What the native transport tells about the TLS connection of a response (wave 19, strand R):
//! [`TlsInfo`] (protocol, cipher, validity and key of the leaf certificate), read once per
//! connection after the handshake and kept per origin in a [`TlsRegistry`], so a response on
//! a pooled connection still has it. Only what a passive check needs: no key material, no
//! issuer, no subject. The certificate is read with a small DER reader (no extra
//! dependency); anything odd leaves the field out.

use std::collections::HashMap;
use std::sync::Mutex;

use super::super::TlsInfo;

/// At most this many origins are remembered; the table is cleared when it grows beyond.
const MAX_ORIGINS: usize = 128;

/// The TLS details of the connections made so far, by `host:port`.
#[derive(Debug, Default)]
pub(super) struct TlsRegistry {
    seen: Mutex<HashMap<String, TlsInfo>>,
}

impl TlsRegistry {
    pub fn record(&self, host: &str, port: u16, info: TlsInfo) {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        if seen.len() >= MAX_ORIGINS {
            seen.clear();
        }
        seen.insert(key(host, port), info);
    }

    /// The details of the last connection to the origin of `url`.
    pub fn lookup(&self, url: &str) -> Option<TlsInfo> {
        let parsed = url::Url::parse(url).ok()?;
        if parsed.scheme() != "https" {
            return None;
        }
        let host = parsed.host_str()?.trim_matches(['[', ']']);
        let port = parsed.port_or_known_default()?;
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key(host, port))
            .cloned()
    }
}

fn key(host: &str, port: u16) -> String {
    format!("{}:{port}", host.to_ascii_lowercase())
}

/// The protocol name as people write it: `TLSv1.3`.
pub(super) fn protocol_name(version: rustls::ProtocolVersion) -> String {
    match version.as_str() {
        Some(name) => name.replace('_', "."),
        None => "unknown".to_owned(),
    }
}

/// Builds the info of a finished handshake from what rustls says and the leaf certificate.
pub(super) fn info_of(
    protocol: Option<rustls::ProtocolVersion>,
    cipher: Option<rustls::SupportedCipherSuite>,
    leaf_der: Option<&[u8]>,
) -> TlsInfo {
    let mut info = TlsInfo {
        protocol: protocol.map(protocol_name),
        cipher: cipher.and_then(|c| c.suite().as_str()).map(str::to_owned),
        ..TlsInfo::default()
    };
    if let Some(facts) = leaf_der.and_then(read_certificate) {
        info.not_before = Some(facts.not_before);
        info.not_after = Some(facts.not_after);
        info.key_algorithm = facts.key_algorithm.map(str::to_owned);
        info.key_bits = facts.key_bits;
    }
    info
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CertFacts {
    pub not_before: i64,
    pub not_after: i64,
    pub key_algorithm: Option<&'static str>,
    pub key_bits: Option<u32>,
}

/// One DER element: tag, content and what follows.
fn tlv(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = bytes.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 || rest.len() < n {
            return None;
        }
        let len = rest[..n]
            .iter()
            .fold(0usize, |l, &b| (l << 8) | usize::from(b));
        (len, &rest[n..])
    };
    if rest.len() < len {
        return None;
    }
    Some((tag, &rest[..len], &rest[len..]))
}

const OID_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_EC: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_ED25519: &[u8] = &[0x2B, 0x65, 0x70];
const OID_ED448: &[u8] = &[0x2B, 0x65, 0x71];
const OID_P256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x22];
const OID_P521: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x23];
const OID_K256: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x0A];

/// Validity and key of a DER certificate; `None` if it cannot be read.
pub(super) fn read_certificate(der: &[u8]) -> Option<CertFacts> {
    let (_, cert, _) = tlv(der)?;
    let (_, tbs, _) = tlv(cert)?;
    let mut rest = tbs;
    let (mut tag, mut content, mut next) = tlv(rest)?;
    if tag == 0xA0 {
        // The explicit version.
        rest = next;
        (tag, content, next) = tlv(rest)?;
    }
    let _ = (tag, content); // serial number
    rest = next;
    // Signature algorithm, issuer.
    for _ in 0..2 {
        let (_, _, next) = tlv(rest)?;
        rest = next;
    }
    let (_, validity, next) = tlv(rest)?;
    rest = next;
    let (t1, v1, after) = tlv(validity)?;
    let (t2, v2, _) = tlv(after)?;
    let not_before = time(t1, v1)?;
    let not_after = time(t2, v2)?;
    // Subject, then the public key.
    let (_, _, next) = tlv(rest)?;
    let (_, spki, _) = tlv(next)?;
    let (key_algorithm, key_bits) = key_of(spki).unwrap_or((None, None));
    Some(CertFacts {
        not_before,
        not_after,
        key_algorithm,
        key_bits,
    })
}

fn key_of(spki: &[u8]) -> Option<(Option<&'static str>, Option<u32>)> {
    let (_, algorithm, after) = tlv(spki)?;
    let (_, key_bits, _) = tlv(after)?;
    let (_, oid, params) = tlv(algorithm)?;
    Some(match oid {
        OID_RSA => (Some("RSA"), rsa_bits(key_bits)),
        OID_EC => {
            let bits = tlv(params).and_then(|(_, curve, _)| match curve {
                OID_P256 | OID_K256 => Some(256),
                OID_P384 => Some(384),
                OID_P521 => Some(521),
                _ => None,
            });
            (Some("EC"), bits)
        }
        OID_ED25519 => (Some("Ed25519"), Some(256)),
        OID_ED448 => (Some("Ed448"), Some(448)),
        _ => (None, None),
    })
}

/// The modulus size of an RSA public key (BIT STRING holding `SEQUENCE { n, e }`).
fn rsa_bits(bit_string: &[u8]) -> Option<u32> {
    let (_unused_bits, key) = bit_string.split_first()?;
    let (_, sequence, _) = tlv(key)?;
    let (tag, modulus, _) = tlv(sequence)?;
    if tag != 0x02 {
        return None;
    }
    let modulus = match modulus {
        [0, rest @ ..] => rest,
        other => other,
    };
    let first = *modulus.first()?;
    let top = 8 - first.leading_zeros();
    Some(u32::try_from(modulus.len() - 1).ok()? * 8 + top)
}

/// UTCTime `YYMMDDHHMMSSZ` or GeneralizedTime `YYYYMMDDHHMMSSZ` as Unix seconds.
fn time(tag: u8, value: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(value).ok()?;
    let text = text.strip_suffix('Z')?;
    let (year, rest) = match tag {
        0x17 if text.len() == 12 => {
            let yy: i64 = text[..2].parse().ok()?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, &text[2..])
        }
        0x18 if text.len() == 14 => (text[..4].parse().ok()?, &text[4..]),
        _ => return None,
    };
    let part = |i: usize| -> Option<i64> { rest.get(i..i + 2)?.parse().ok() };
    let (month, day, hour, minute, second) = (part(0)?, part(2)?, part(4)?, part(6)?, part(8)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, KeyPair, date_time_ymd};

    fn cert(key: &KeyPair, from: (i32, u8, u8), to: (i32, u8, u8)) -> Vec<u8> {
        let mut params = CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
        params.not_before = date_time_ymd(from.0, from.1, from.2);
        params.not_after = date_time_ymd(to.0, to.1, to.2);
        params.self_signed(key).unwrap().der().to_vec()
    }

    #[test]
    fn dates_and_an_ec_key_are_read() {
        let key = KeyPair::generate().unwrap();
        let der = cert(&key, (2024, 3, 1), (2049, 12, 31));
        let facts = read_certificate(&der).unwrap();
        // 2024-03-01T00:00:00Z and 2049-12-31T00:00:00Z.
        assert_eq!(facts.not_before, 1_709_251_200);
        assert_eq!(facts.not_after, 2_524_521_600);
        assert_eq!(facts.key_algorithm, Some("EC"));
        assert_eq!(facts.key_bits, Some(256));
    }

    #[test]
    fn an_ed25519_key_is_named() {
        let key = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let facts = read_certificate(&cert(&key, (2020, 1, 1), (2030, 1, 1))).unwrap();
        assert_eq!(facts.key_algorithm, Some("Ed25519"));
        // A UTCTime up to 2049 and a GeneralizedTime from 2050 on both parse.
        let far = read_certificate(&cert(&key, (2020, 1, 1), (2060, 1, 1))).unwrap();
        assert_eq!(far.not_after, 2_840_140_800);
    }

    #[test]
    fn odd_input_is_nothing_not_a_panic() {
        assert_eq!(read_certificate(&[]), None);
        assert_eq!(
            read_certificate(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]),
            None
        );
        let key = KeyPair::generate().unwrap();
        let der = cert(&key, (2024, 3, 1), (2030, 1, 1));
        for cut in [1, 10, 30, der.len() / 2] {
            let _ = read_certificate(&der[..cut]);
        }
    }

    #[test]
    fn rsa_sizes_count_the_modulus_bits() {
        // BIT STRING (0 unused) over SEQUENCE { INTEGER 0x00FF.., INTEGER 3 } with a 2048-bit n.
        let mut n = vec![0u8];
        n.extend(std::iter::repeat_n(0xFFu8, 256));
        let mut seq = vec![0x02, 0x82, 0x01, 0x01];
        seq.extend(&n);
        seq.extend([0x02, 0x01, 0x03]);
        let mut key = vec![0x30, 0x82, (seq.len() >> 8) as u8, seq.len() as u8];
        key.extend(&seq);
        let mut bits = vec![0u8];
        bits.extend(&key);
        assert_eq!(rsa_bits(&bits), Some(2048));
    }

    #[test]
    fn the_registry_keeps_one_entry_per_origin() {
        let registry = TlsRegistry::default();
        registry.record("Example.com", 443, TlsInfo::default());
        assert!(registry.lookup("https://example.com/x").is_some());
        assert!(registry.lookup("https://example.com:8443/x").is_none());
        assert!(registry.lookup("http://example.com/x").is_none());
    }
}
