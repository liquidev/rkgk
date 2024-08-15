use std::fmt;

use base64::Engine;

pub fn serialize(f: &mut fmt::Formatter<'_>, prefix: &str, bytes: &[u8; 32]) -> fmt::Result {
    f.write_str(prefix)?;
    let mut buffer = [b'0'; 43];
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode_slice(bytes, &mut buffer)
        .unwrap();
    f.write_str(std::str::from_utf8(&buffer).unwrap())?;
    Ok(())
}

pub struct InvalidId;

pub fn deserialize(s: &str, prefix: &str) -> Result<[u8; 32], InvalidId> {
    let mut bytes = [0; 32];
    let b64 = s.strip_prefix(prefix).ok_or(InvalidId)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode_slice(b64, &mut bytes)
        .map_err(|_| InvalidId)?;
    if decoded != bytes.len() {
        return Err(InvalidId);
    }
    Ok(bytes)
}
