use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use base64::{engine::general_purpose::STANDARD, Engine as _};

const KEYRING_SERVICE: &str = "verdant-desktop";
const KEYRING_USER: &str = "encryption-key";

static KEY_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fallback_key_path() -> std::path::PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    path.push("com.cns-studios.verdant");
    path.push("encryption.key");
    path
}

fn read_fallback_key() -> Result<Option<Vec<u8>>, String> {
    let path = fallback_key_path();
    match std::fs::read_to_string(&path) {
        Ok(hex_key) => hex::decode(hex_key.trim()).map(Some).map_err(|e| e.to_string()),
        Err(_) => Ok(None),
    }
}

fn write_fallback_key(key: &[u8]) -> Result<(), String> {
    let path = fallback_key_path();
    let path_str = path.display().to_string();
    let result = (|| -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, hex::encode(key)).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    })();
    if result.is_ok() {
        log::warn!("keyring unavailable — storing encryption key in plain file at {}", path_str);
    }
    result
}

fn keyring_key() -> Result<Option<Vec<u8>>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("keyring entry error: {}", e))?;
    match entry.get_password() {
        Ok(hex_key) => hex::decode(&hex_key).map(Some).map_err(|e| e.to_string()),
        Err(e) => Err(format!("keyring get error: {}", e)),
    }
}

fn get_or_create_key() -> Result<Vec<u8>, String> {
    let _guard = KEY_GUARD.lock().unwrap_or_else(|p| p.into_inner());

    if let Some(key) = read_fallback_key()? {
        return Ok(key);
    }

    if let Ok(Some(key)) = keyring_key() {
        return Ok(key);
    }

    let mut key_bytes = vec![0u8; 32];
    OsRng.fill_bytes(&mut key_bytes);

    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        Ok(entry) if entry.set_password(&hex::encode(&key_bytes)).is_ok() => Ok(key_bytes),
        _ => {
            write_fallback_key(&key_bytes)?;
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
        .map_err(|_| "Decryption failed - wrong key or corrupted data".to_string())?;

    String::from_utf8(plaintext).map_err(|e| e.to_string())
}