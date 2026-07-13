use aes_gcm::{
    aead::{Aead, Generate, KeyInit, Nonce},
    Aes256Gcm,
};
use anyhow::{anyhow, Context, Result};
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Size of AES-256-GCM nonce in bytes (96 bits / 12 bytes is standard)
const NONCE_SIZE: usize = 12;

/// Encrypted credential storage format
#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedCredentials {
    /// Format version for future compatibility
    pub version: u32,
    /// Base64-encoded salt used for key derivation
    pub salt: String,
    /// Map of account name to encrypted token
    pub credentials: HashMap<String, EncryptedToken>,
}

/// A single encrypted token with its nonce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedToken {
    /// Base64-encoded nonce (12 bytes)
    pub nonce: String,
    /// Base64-encoded ciphertext
    pub ciphertext: String,
}

impl Default for EncryptedCredentials {
    fn default() -> Self {
        Self {
            version: 1,
            salt: String::new(),
            credentials: HashMap::new(),
        }
    }
}

/// Derive an encryption key from machine-specific identifiers.
/// Uses Argon2 for key derivation to resist brute-force attacks.
pub fn derive_key() -> Result<[u8; 32]> {
    let machine_id = machine_uid::get().map_err(|e| anyhow!("Failed to get machine ID: {}", e))?;
    let username = whoami::username().unwrap_or_else(|_| "unknown".to_string());

    // Combine machine ID and username as the password
    let password = format!("{}:{}", machine_id, username);

    // Use a fixed salt derived from machine ID for deterministic key generation
    // This allows the same key to be derived across runs
    let salt_string = SaltString::encode_b64(machine_id.as_bytes())
        .map_err(|e| anyhow!("Failed to encode salt: {}", e))?;

    let argon2 = Argon2::default();

    // Hash the password to get a 32-byte key
    let hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|e| anyhow!("Failed to hash password: {}", e))?;

    // Extract the 32-byte hash
    let hash_bytes = hash.hash.ok_or_else(|| anyhow!("Hash output is missing"))?;

    let mut key = [0u8; 32];
    key.copy_from_slice(&hash_bytes.as_bytes()[..32]);

    Ok(key)
}

/// Encrypt plaintext using AES-256-GCM
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(String, String)> {
    let cipher = Aes256Gcm::new(key.into());

    // Generate a random nonce. `Generate` draws from the OS RNG (aes-gcm's
    // `getrandom` feature, on by default) and replaces the `aead::OsRng` that
    // aead 0.6 removed. `try_generate` rather than `generate` because the latter
    // panics if the system RNG fails. Still 12 bytes: the stored format is unchanged.
    let nonce = Nonce::<Aes256Gcm>::try_generate()
        .map_err(|e| anyhow!("Failed to generate nonce from system RNG: {}", e))?;

    // Encrypt the plaintext
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Encode as base64 for storage
    let nonce_b64 = BASE64.encode(nonce);
    let ciphertext_b64 = BASE64.encode(ciphertext);

    Ok((nonce_b64, ciphertext_b64))
}

/// Decrypt ciphertext using AES-256-GCM
pub fn decrypt(ciphertext_b64: &str, nonce_b64: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());

    // Decode from base64
    let nonce_bytes = BASE64
        .decode(nonce_b64)
        .context("Failed to decode nonce from base64")?;
    let ciphertext = BASE64
        .decode(ciphertext_b64)
        .context("Failed to decode ciphertext from base64")?;

    if nonce_bytes.len() != NONCE_SIZE {
        return Err(anyhow!(
            "Invalid nonce size: expected {}, got {}",
            NONCE_SIZE,
            nonce_bytes.len()
        ));
    }

    // `Array::from_slice` is deprecated in hybrid-array; the length is already
    // checked above, so TryFrom cannot fail here.
    let nonce = Nonce::<Aes256Gcm>::try_from(nonce_bytes.as_slice())
        .map_err(|_| anyhow!("Invalid nonce size: expected {}", NONCE_SIZE))?;

    // Decrypt
    let plaintext = cipher
        .decrypt(&nonce, ciphertext.as_ref())
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-version known-answer test. This nonce/ciphertext pair was produced by
    /// the aes-gcm 0.10 build (the version that wrote users' existing
    /// `~/.atlassian-cli/credentials.enc` files). Decrypting it here proves the
    /// aes-gcm 0.11 upgrade did not change the on-disk format and that stored
    /// credentials still open. Every other test in this module is a same-process
    /// round-trip and would pass even if the format silently changed.
    #[test]
    fn decrypts_ciphertext_written_by_aes_gcm_0_10() {
        let key = [7u8; 32];
        let nonce_b64 = "Rm9oYDGUcR47yGPD";
        let ciphertext_b64 = "iwXkjpxTMOS8N/vvR/y0Yvt3G7fE0GW4sl7KvlyPIJ3rW50U/81e";

        let plaintext = decrypt(ciphertext_b64, nonce_b64, &key)
            .expect("aes-gcm 0.11 must decrypt ciphertext written by 0.10");
        assert_eq!(plaintext, "hunter2-atlassian-token");
    }

    #[test]
    fn nonce_is_12_bytes() {
        // The stored format depends on this; a change would orphan existing files.
        let (nonce_b64, _) = encrypt("x", &[0u8; 32]).unwrap();
        assert_eq!(BASE64.decode(nonce_b64).unwrap().len(), NONCE_SIZE);
        assert_eq!(NONCE_SIZE, 12);
    }

    #[test]
    fn test_derive_key_deterministic() {
        // Key derivation should be deterministic for the same machine/user
        let key1 = derive_key().expect("Failed to derive key");
        let key2 = derive_key().expect("Failed to derive key");
        assert_eq!(key1, key2, "Key derivation should be deterministic");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_key().expect("Failed to derive key");
        let plaintext = "my-secret-token-12345";

        let (nonce, ciphertext) = encrypt(plaintext, &key).expect("Encryption failed");

        // Verify encrypted data is different from plaintext
        assert_ne!(ciphertext, plaintext);
        assert!(!ciphertext.contains("secret"));

        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("Decryption failed");
        assert_eq!(decrypted, plaintext, "Decrypted text should match original");
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let key = derive_key().expect("Failed to derive key");
        let plaintext = "same-plaintext";

        // Encrypt the same plaintext twice
        let (nonce1, ciphertext1) = encrypt(plaintext, &key).expect("Encryption failed");
        let (nonce2, ciphertext2) = encrypt(plaintext, &key).expect("Encryption failed");

        // Nonces should be different (random)
        assert_ne!(nonce1, nonce2, "Nonces should be randomly generated");

        // Ciphertexts should be different (because nonces are different)
        assert_ne!(
            ciphertext1, ciphertext2,
            "Ciphertexts should differ with different nonces"
        );

        // Both should decrypt to the same plaintext
        assert_eq!(decrypt(&ciphertext1, &nonce1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&ciphertext2, &nonce2, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = derive_key().expect("Failed to derive key");
        let mut key2 = key1;
        key2[0] ^= 0xFF; // Flip bits to create a different key

        let plaintext = "secret-data";
        let (nonce, ciphertext) = encrypt(plaintext, &key1).expect("Encryption failed");

        // Decryption with wrong key should fail
        let result = decrypt(&ciphertext, &nonce, &key2);
        assert!(result.is_err(), "Decryption with wrong key should fail");
    }

    #[test]
    fn test_decrypt_with_wrong_nonce_fails() {
        let key = derive_key().expect("Failed to derive key");
        let plaintext = "secret-data";

        let (_, ciphertext) = encrypt(plaintext, &key).expect("Encryption failed");
        let (wrong_nonce, _) = encrypt("other", &key).expect("Encryption failed");

        // Decryption with wrong nonce should fail
        let result = decrypt(&ciphertext, &wrong_nonce, &key);
        assert!(result.is_err(), "Decryption with wrong nonce should fail");
    }

    #[test]
    fn test_encrypted_credentials_serialization() {
        let mut creds = EncryptedCredentials {
            salt: "test-salt".to_string(),
            ..Default::default()
        };
        creds.credentials.insert(
            "account1".to_string(),
            EncryptedToken {
                nonce: "nonce-b64".to_string(),
                ciphertext: "cipher-b64".to_string(),
            },
        );

        let json = serde_json::to_string(&creds).expect("Serialization failed");
        let deserialized: EncryptedCredentials =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.salt, "test-salt");
        assert_eq!(deserialized.credentials.len(), 1);
    }
}
