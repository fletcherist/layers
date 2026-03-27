// ---------------------------------------------------------------------------
// src/share_id.rs — Base62 project ID generation (Figma-style)
// ---------------------------------------------------------------------------
// Generates 22-character base62 strings from UUID v4 bytes.
// Example: "QaoQDz0jW3WpoEy9hwPRDF"
// Used as both the URL slug (https://layers.audio/projects/{id})
// and the internal SurrealDB project ID.

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Generate a new 22-character base62 share ID from a UUID v4.
pub fn generate() -> String {
    uuid_to_base62(uuid::Uuid::new_v4())
}

fn uuid_to_base62(uuid: uuid::Uuid) -> String {
    let mut n = u128::from_be_bytes(*uuid.as_bytes());
    let mut result = vec![b'0'; 22];
    for i in (0..22).rev() {
        result[i] = BASE62[(n % 62) as usize];
        n /= 62;
    }
    String::from_utf8(result).expect("base62 chars are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_length() {
        let id = generate();
        assert_eq!(id.len(), 22, "share ID must be 22 chars");
    }

    #[test]
    fn test_generate_base62_chars() {
        let id = generate();
        for c in id.chars() {
            assert!(
                c.is_ascii_alphanumeric(),
                "char '{c}' is not base62"
            );
        }
    }

    #[test]
    fn test_uniqueness() {
        let a = generate();
        let b = generate();
        assert_ne!(a, b, "two generated IDs should differ");
    }
}
