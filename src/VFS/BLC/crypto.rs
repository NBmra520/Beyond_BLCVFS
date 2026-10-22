use super::error::{BlcError, Result};
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::ChaCha20;
use base64::{Engine as _, engine::general_purpose};

const CHACHA_KEY_BASE64: &str = "6VsxesT4KFadI6hr8nHctT6Eb6dckk1nHbqOOPTKUuE=";

pub const VFS_PROTO_VERSION: i32 = 3;

pub struct Decryptor {
    key: [u8; 32],
}

impl Decryptor {
    pub fn new() -> Result<Self> {
        let key = general_purpose::STANDARD
            .decode(CHACHA_KEY_BASE64)
            .map_err(|e| BlcError::DecryptionFailed(format!("Invalid base64 key: {}", e)))?;

        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| BlcError::DecryptionFailed("Invalid key length".to_string()))?;

        Ok(Self { key })
    }

    pub fn decrypt_blc(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(BlcError::DecryptionFailed(
                "Data too short to contain nonce".to_string(),
            ));
        }

        let nonce = &encrypted_data[..12];
        let ciphertext = &encrypted_data[12..];

        let mut cipher = ChaCha20::new(&self.key.into(), nonce.into());
        cipher.seek(64);

        let mut decrypted = ciphertext.to_vec();
        cipher.apply_keystream(&mut decrypted);

        Ok(decrypted)
    }

    pub fn decrypt_file(&self, iv_seed: i64, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&VFS_PROTO_VERSION.to_le_bytes());
        nonce[4..12].copy_from_slice(&iv_seed.to_le_bytes());

        let mut cipher = ChaCha20::new(&self.key.into(), (&nonce).into());
        cipher.seek(64);

        let mut decrypted = encrypted_data.to_vec();
        cipher.apply_keystream(&mut decrypted);

        Ok(decrypted)
    }
}

impl Default for Decryptor {
    fn default() -> Self {
        Self::new().expect("Failed to create decryptor")
    }
}
