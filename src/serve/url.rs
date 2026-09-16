//! Percent-encoding, in the two directions the server needs it.
//!
//! Only the request path is involved, so this deliberately does not try to be a
//! general URL library: it decodes what a browser sent and encodes a file name
//! back into a link that will come back naming the same file.

use std::fmt::Write as _;

/// Decode a percent-encoded path.
///
/// Returns `None` if the escapes are malformed or the result is not UTF-8,
/// which for a request path means it cannot name a file this server would
/// serve.
#[must_use]
pub(super) fn decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = hex_value(*bytes.get(index + 1)?)?;
                let low = hex_value(*bytes.get(index + 2)?)?;

                out.push(high * 16 + low);
                index += 3;
            }

            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8(out).ok()
}

/// Encode one path segment for use in a link.
///
/// Everything outside the unreserved set is escaped. That is more than strictly
/// required — a file name is allowed to contain, say, a comma — but escaping it
/// is always correct, and guessing which delimiters a browser will reinterpret
/// is not.
#[must_use]
pub(super) fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());

    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }

            byte => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }

    out
}

/// The value of one hexadecimal digit.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escapes_and_plain_text() {
        assert_eq!(decode("/a%20b/c.adoc").as_deref(), Some("/a b/c.adoc"));
        assert_eq!(decode("/plain").as_deref(), Some("/plain"));
        assert_eq!(decode("/caf%C3%A9").as_deref(), Some("/café"));
    }

    #[test]
    fn rejects_malformed_escapes() {
        assert_eq!(decode("/a%2"), None);
        assert_eq!(decode("/a%zz"), None);
        assert_eq!(decode("/a%FF"), None);
    }

    #[test]
    fn round_trips_a_segment() {
        for segment in ["plain.adoc", "a b.adoc", "caf\u{e9}", "100%", "a#b?c"] {
            assert_eq!(decode(&encode_segment(segment)).as_deref(), Some(segment));
        }
    }
}
