//! Lipila webhook signature verification (Standard Webhooks scheme):
//! `v1,<base64 HMAC-SHA256("<webhook-id>.<timestamp>.<raw body>")>`.

use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Base64 decoder that, like the browser `atob`, accepts missing padding.
const FORGIVING_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Decode a `whsec_…` signing secret into its 32-byte key.
pub fn decode_secret(secret: &str) -> Option<Vec<u8>> {
    let encoded = secret.trim();
    let encoded = encoded.strip_prefix("whsec_").unwrap_or(encoded);
    let bytes = FORGIVING_BASE64.decode(encoded).ok()?;
    (bytes.len() == 32).then_some(bytes)
}

/// Returns true when any `v1` signature in the header matches the payload.
pub fn verify_signature(
    secret: &str,
    webhook_id: &str,
    timestamp: &str,
    raw_body: &[u8],
    signature_header: &str,
) -> bool {
    let Some(key) = decode_secret(secret) else {
        return false;
    };
    for signature in signature_header.split_whitespace() {
        let Some((version, encoded)) = signature.split_once(',') else {
            continue;
        };
        if version != "v1" || encoded.is_empty() {
            continue;
        }
        let Ok(expected) = FORGIVING_BASE64.decode(encoded) else {
            continue;
        };
        let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(&key) else {
            return false;
        };
        mac.update(format!("{webhook_id}.{timestamp}.").as_bytes());
        mac.update(raw_body);
        // `verify_slice` compares in constant time.
        if mac.verify_slice(&expected).is_ok() {
            return true;
        }
    }
    false
}
