//! Bounded artifact chunk results for `artifact.lifecycle.v1`.
use serde::{Deserialize, Serialize};

pub const MAX_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactChunk {
    pub artifact_id: String,
    pub sha256: String,
    pub media_type: String,
    pub byte_length: u64,
    pub offset: u64,
    pub data_base64: String,
}

impl ArtifactChunk {
    /// Validates chunk bounds and request binding, returning decoded bytes.
    /// The caller assembles chunks and verifies the final SHA-256.
    pub fn validate(
        &self,
        artifact_id: &str,
        sha256: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        if self.artifact_id != artifact_id || self.sha256 != sha256 || self.offset != offset {
            return Err(
                "artifact chunk does not match the requested identity, digest and offset".into(),
            );
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("artifact chunk sha256 must be lowercase hexadecimal".into());
        }
        if self.media_type.is_empty()
            || self.media_type.chars().count() > 127
            || self.byte_length > MAX_ARTIFACT_BYTES
        {
            return Err("artifact chunk metadata exceeds its bounds".into());
        }
        let bytes = decode_base64(&self.data_base64)?;
        if max_bytes == 0
            || max_bytes > MAX_CHUNK_BYTES
            || bytes.len() > max_bytes
            || bytes.len() > MAX_CHUNK_BYTES
            || offset > self.byte_length
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > self.byte_length)
        {
            return Err("artifact chunk exceeds requested or declared bounds".into());
        }
        if self.byte_length == 0 && (offset != 0 || !bytes.is_empty()) {
            return Err("empty artifacts must return one empty chunk at offset zero".into());
        }
        if self.byte_length > 0 && bytes.is_empty() {
            return Err("non-empty artifacts must make progress".into());
        }
        Ok(bytes)
    }
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if input.len() % 4 != 0 || input.len() > MAX_CHUNK_BYTES.div_ceil(3) * 4 {
        return Err("artifact chunk base64 has invalid length".into());
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let bytes = input.as_bytes();
    for (index, block) in bytes.chunks_exact(4).enumerate() {
        let final_block = index + 1 == bytes.len() / 4;
        let a = sextet(block[0]).ok_or("artifact chunk base64 is invalid")? as u32;
        let b = sextet(block[1]).ok_or("artifact chunk base64 is invalid")? as u32;
        let c = if block[2] == b'=' {
            0
        } else {
            sextet(block[2]).ok_or("artifact chunk base64 is invalid")? as u32
        };
        let d = if block[3] == b'=' {
            0
        } else {
            sextet(block[3]).ok_or("artifact chunk base64 is invalid")? as u32
        };
        if (!final_block && (block[2] == b'=' || block[3] == b'='))
            || (block[2] == b'=' && block[3] != b'=')
            || (block[2] == b'=' && b & 0x0f != 0)
            || (block[3] == b'=' && block[2] != b'=' && c & 0x03 != 0)
        {
            return Err("artifact chunk base64 padding is invalid".into());
        }
        let value = (a << 18) | (b << 12) | (c << 6) | d;
        out.push((value >> 16) as u8);
        if block[2] != b'=' {
            out.push((value >> 8) as u8);
        }
        if block[3] != b'=' {
            out.push(value as u8);
        }
    }
    Ok(out)
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_binds_bounded_chunks() {
        let chunk = ArtifactChunk {
            artifact_id: "a1".into(),
            sha256: "a".repeat(64),
            media_type: "text/plain".into(),
            byte_length: 2,
            offset: 0,
            data_base64: "aGk=".into(),
        };
        assert_eq!(chunk.validate("a1", &"a".repeat(64), 0, 2).unwrap(), b"hi");
        assert!(chunk.validate("a1", &"a".repeat(64), 0, 1).is_err());
        assert!(chunk.validate("other", &"a".repeat(64), 0, 2).is_err());
    }

    #[test]
    fn rejects_bad_base64_and_ranges() {
        let mut chunk = ArtifactChunk {
            artifact_id: "a1".into(),
            sha256: "a".repeat(64),
            media_type: "text/plain".into(),
            byte_length: 1,
            offset: 0,
            data_base64: "aGk=".into(),
        };
        assert!(chunk.validate("a1", &"a".repeat(64), 0, 2).is_err());
        chunk.data_base64 = "!!!!".into();
        assert!(chunk.validate("a1", &"a".repeat(64), 0, 2).is_err());
    }

    #[test]
    fn accepts_exactly_256_kib_and_matches_schema_encoded_limit() {
        let encoded = format!("{}AA==", "A".repeat(349_524));
        let chunk = ArtifactChunk {
            artifact_id: "a1".into(),
            sha256: "a".repeat(64),
            media_type: "application/octet-stream".into(),
            byte_length: MAX_CHUNK_BYTES as u64,
            offset: 0,
            data_base64: encoded,
        };
        assert_eq!(
            chunk
                .validate("a1", &"a".repeat(64), 0, MAX_CHUNK_BYTES)
                .unwrap()
                .len(),
            MAX_CHUNK_BYTES
        );
        let schema: serde_json::Value =
            serde_json::from_str(crate::schemas::ARTIFACT_LIFECYCLE).unwrap();
        assert_eq!(
            schema["$defs"]["chunkResult"]["properties"]["dataBase64"]["maxLength"],
            chunk.data_base64.len()
        );
    }
}
