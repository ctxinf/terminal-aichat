use super::AuthConfig;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};

pub fn resolve_auth(api_key: &str, entries: &std::collections::HashMap<String, AuthConfig>) -> Result<String, String> {
    let Some(reference) = api_key.strip_prefix('$') else {
        return Ok(api_key.to_owned());
    };
    let variable = if let Some(braced) = reference.strip_prefix('{') {
        braced.strip_suffix('}').ok_or("invalid apiKey environment reference")?
    } else {
        reference
    };
    if variable.is_empty() || !variable.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("invalid apiKey environment reference".into());
    }
    match std::env::var(variable) {
        Ok(value) if !value.is_empty() => return Ok(value),
        Ok(_) | Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!("credential variable '{variable}' is empty or invalid Unicode"));
        }
        Err(std::env::VarError::NotPresent) => {}
    }
    let mut matches = entries.iter().filter(|(_, auth)| auth.key.as_deref() == Some(variable));
    let (id, auth) = matches
        .next()
        .ok_or_else(|| format!("no credential for variable '{variable}'"))?;
    if matches.next().is_some() {
        return Err(format!("multiple auth entries for variable '{variable}'"));
    }
    let value = auth
        .value
        .as_ref()
        .ok_or_else(|| format!("auth.{id}: no credential value for '{variable}'"))?;
    let resolved = if let Some(password) = &auth.encryption {
        decrypt(value, password).map_err(|_| format!("auth.{id}: invalid encrypted credential or password"))?
    } else {
        value.clone()
    };
    if resolved.is_empty() {
        return Err(format!("auth.{id}: credential is empty"));
    }
    Ok(resolved)
}

fn decrypt(value: &str, password: &str) -> Result<String, Box<dyn std::error::Error>> {
    let packet = STANDARD.decode(value.strip_prefix("v1:").ok_or("unsupported credential version")?)?;
    if packet.len() < 45 {
        return Err("invalid credential packet".into());
    }
    let params = argon2::Params::new(65536, 3, 1, Some(32)).map_err(|_| "invalid KDF parameters")?;
    let kdf = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; 32];
    kdf.hash_password_into(password.as_bytes(), &packet[..16], &mut key)
        .map_err(|_| "credential derivation failed")?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "invalid key")?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&packet[16..28]),
            Payload {
                msg: &packet[28..],
                aad: b"terminal-aichat/auth/v1",
            },
        )
        .map_err(|_| "invalid credential")?;
    Ok(String::from_utf8(plain)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[test]
    fn literal_and_encrypted_credentials() {
        let variable = format!("TERMINAL_AICHT_TEST_UNSET_{}", std::process::id());
        let params = argon2::Params::new(65536, 3, 1, Some(32)).unwrap();
        let kdf = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let salt = [7u8; 16];
        let nonce = [9u8; 12];
        let mut key = [0u8; 32];
        kdf.hash_password_into(b"test-password", &salt, &mut key).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: b"test-key",
                    aad: b"terminal-aichat/auth/v1",
                },
            )
            .unwrap();
        let packet = [salt.as_slice(), nonce.as_slice(), &encrypted].concat();
        let mut entries = HashMap::from([(
            "account".into(),
            AuthConfig {
                key: Some(variable.clone()),
                value: Some(format!("v1:{}", STANDARD.encode(packet))),
                encryption: Some("test-password".into()),
            },
        )]);
        assert_eq!(resolve_auth(&format!("${variable}"), &entries).unwrap(), "test-key");
        assert_eq!(resolve_auth(&format!("${{{variable}}}"), &entries).unwrap(), "test-key");
        entries.get_mut("account").unwrap().encryption = Some("wrong-password".into());
        assert!(resolve_auth(&format!("${variable}"), &entries).is_err());
        assert_eq!(
            resolve_auth("!literal-not-command", &entries).unwrap(),
            "!literal-not-command"
        );
        assert!(decrypt("v2:AAAA", "password").is_err());
        assert!(decrypt("v1:AAAA", "password").is_err());
    }
}
