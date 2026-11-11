use super::ApiKeyConfig;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, Payload, rand_core::RngCore},
};
use base64::{Engine, engine::general_purpose::STANDARD};

fn cipher(password: &str, salt: &[u8]) -> Result<Aes256Gcm, String> {
    let params = argon2::Params::new(65536, 3, 1, Some(32)).map_err(|_| "invalid KDF parameters")?;
    let kdf = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; 32];
    kdf.hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| "credential derivation failed")?;
    Aes256Gcm::new_from_slice(&key).map_err(|_| "invalid credential key".into())
}

pub fn encrypt_api_key(value: &str) -> Result<ApiKeyConfig, String> {
    if value.is_empty() {
        return Err("API key must not be empty".into());
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    let mut password = [0u8; 32];
    for bytes in [salt.as_mut_slice(), nonce.as_mut_slice(), password.as_mut_slice()] {
        OsRng
            .try_fill_bytes(bytes)
            .map_err(|_| "credential randomness unavailable")?;
    }
    let password = STANDARD.encode(password);
    let encrypted = cipher(&password, &salt)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: value.as_bytes(),
                aad: b"terminal-aichat/auth/v1",
            },
        )
        .map_err(|_| "credential encryption failed")?;
    let packet = [salt.as_slice(), nonce.as_slice(), &encrypted].concat();
    Ok(ApiKeyConfig {
        value: format!("v1:{}", STANDARD.encode(packet)),
        encryption: Some(password),
    })
}

pub fn resolve_api_key(api_key: &ApiKeyConfig) -> Result<String, String> {
    if api_key.value.is_empty() {
        return Err("API key must not be empty".into());
    }
    let Some(password) = api_key.encryption.as_deref().filter(|password| !password.is_empty()) else {
        return Ok(api_key.value.clone());
    };
    let invalid = || "invalid encrypted API key or password".to_string();
    let packet = STANDARD
        .decode(api_key.value.strip_prefix("v1:").ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    if packet.len() < 45 {
        return Err(invalid());
    }
    let plain = cipher(password, &packet[..16])?
        .decrypt(
            Nonce::from_slice(&packet[16..28]),
            Payload {
                msg: &packet[28..],
                aad: b"terminal-aichat/auth/v1",
            },
        )
        .map_err(|_| invalid())?;
    let plain = String::from_utf8(plain).map_err(|_| invalid())?;
    if plain.is_empty() {
        return Err(invalid());
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_credentials_roundtrip_and_tampering() {
        let mut key = encrypt_api_key("test-secret").unwrap();
        assert!(!key.value.contains("test-secret"));
        assert_eq!(resolve_api_key(&key).unwrap(), "test-secret");
        assert_ne!(encrypt_api_key("test-secret").unwrap().value, key.value);
        assert!(!format!("{key:?}").contains(&key.value));
        key.encryption = Some("wrong-password".into());
        assert!(resolve_api_key(&key).is_err());
        key.value = "v1:AAAA".into();
        assert!(resolve_api_key(&key).is_err());
        assert!(encrypt_api_key("").is_err());
        assert_eq!(
            resolve_api_key(&ApiKeyConfig {
                value: "plain".into(),
                encryption: Some(String::new())
            })
            .unwrap(),
            "plain"
        );
        assert_eq!(
            resolve_api_key(&ApiKeyConfig {
                value: "$literal".into(),
                encryption: None
            })
            .unwrap(),
            "$literal"
        );
    }
}
