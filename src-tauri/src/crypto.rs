use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use base64::{engine::general_purpose::STANDARD, Engine as _};

const KEYRING_SERVICE: &str = "verdant-desktop";
const KEYRING_USER: &str = "encryption-key";



fn get_or_create_key() -> Result<Vec<u8>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| e.to_string())?;

    match entry.get_password() {
        Ok(hex_key) => {
            hex::decode(&hex_key).map_err(|e| e.to_string())
        }
        Err(_) => {
            
            let mut key_bytes = vec![0u8; 32];
            OsRng.fill_bytes(&mut key_bytes);
            let hex_key = hex::encode(&key_bytes);
            entry.set_password(&hex_key).map_err(|e| e.to_string())?;
            Ok(key_bytes)
        }
    }
}

pub fn encrypt_password(plaintext: &str) -> Result<String, String> {
    let key_bytes = get_or_create_key()?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;

    
    let result = format!(
        "{}:{}",
        STANDARD.encode(nonce_bytes),
        STANDARD.encode(ciphertext)
    );
    Ok(result)
}

pub fn decrypt_password(encoded: &str) -> Result<String, String> {
    let key_bytes = get_or_create_key()?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let parts: Vec<&str> = encoded.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err("Invalid encrypted password format".to_string());
    }

    let nonce_bytes = STANDARD.decode(parts[0]).map_err(|e| e.to_string())?;
    let ciphertext = STANDARD.decode(parts[1]).map_err(|e| e.to_string())?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_slice())
        .map_err(|_| "Decryption failed — wrong key or corrupted data".to_string())?;

    String::from_utf8(plaintext).map_err(|e| e.to_string())
}
