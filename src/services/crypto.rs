//! 本地凭据静态加密（AES-256-GCM）
//!
//! 密钥以 `0600` 权限保存在配置目录下的 `.secret.key` 中。
//! 密文统一带 `enc:v1:` 前缀；读取历史明文时自动向后兼容，并在下次保存或启动迁移时升级为密文。

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;

use crate::models::{AppSettings, ServerConfig};

const SECRET_PREFIX: &str = "enc:v1:";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// 进程内缓存的主密钥，避免每次读写配置都重复读盘
static MASTER_KEY_CACHE: OnceLock<[u8; KEY_LEN]> = OnceLock::new();

/// 字符串是否已经是加密后的格式
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(SECRET_PREFIX)
}

/// 从指定密钥文件加载或生成 32 字节主密钥
pub fn load_or_create_key_at(path: &Path) -> Result<[u8; KEY_LEN]> {
    if path.exists() {
        let encoded = fs::read_to_string(path).context("无法读取本地加密密钥文件")?;
        let raw = BASE64
            .decode(encoded.trim())
            .context("本地加密密钥格式无效")?;
        if raw.len() != KEY_LEN {
            return Err(anyhow!(
                "本地加密密钥长度无效：期望 {} 字节，实际 {} 字节",
                KEY_LEN,
                raw.len()
            ));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&raw);
        return Ok(key);
    }

    let mut key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    let encoded = BASE64.encode(key);
    crate::services::storage::write_private_file(path, encoded.as_bytes())
        .context("无法写入本地加密密钥文件")?;
    Ok(key)
}

fn master_key() -> Result<[u8; KEY_LEN]> {
    if let Some(key) = MASTER_KEY_CACHE.get() {
        return Ok(*key);
    }
    let key_path = crate::services::storage::get_config_dir()?.join(".secret.key");
    let key = load_or_create_key_at(&key_path)?;
    let _ = MASTER_KEY_CACHE.set(key);
    Ok(key)
}

/// 使用指定 32 字节密钥加密单个字符串；空字符串原样返回，已加密值不重复加密
pub fn encrypt_secret_with_key(value: &str, key: &[u8; KEY_LEN]) -> Result<String> {
    if value.is_empty() || is_encrypted(value) {
        return Ok(value.to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("初始化加密器失败: {e}"))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|e| anyhow!("加密凭据失败: {e}"))?;

    let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(format!("{SECRET_PREFIX}{}", BASE64.encode(payload)))
}

/// 使用指定 32 字节密钥解密单个字符串；未带 `enc:v1:` 前缀的历史明文直接透传
pub fn decrypt_secret_with_key(value: &str, key: &[u8; KEY_LEN]) -> Result<String> {
    let Some(encoded) = value.strip_prefix(SECRET_PREFIX) else {
        return Ok(value.to_string());
    };
    let payload = BASE64.decode(encoded).context("无法解码加密凭据")?;
    if payload.len() <= NONCE_LEN {
        return Err(anyhow!("加密凭据载荷长度不足"));
    }
    let (nonce_bytes, ciphertext) = payload.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("初始化解密器失败: {e}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|e| anyhow!("解密凭据失败: {e}"))?;
    String::from_utf8(plaintext).context("解密后的凭据不是有效的 UTF-8")
}

fn encrypt_opt_with_key(slot: &mut Option<String>, key: &[u8; KEY_LEN]) -> Result<()> {
    if let Some(value) = slot.as_deref() {
        if !value.is_empty() {
            *slot = Some(encrypt_secret_with_key(value, key)?);
        }
    }
    Ok(())
}

fn decrypt_opt_with_key(slot: &mut Option<String>, key: &[u8; KEY_LEN]) -> Result<()> {
    if let Some(value) = slot.as_deref() {
        if !value.is_empty() {
            *slot = Some(decrypt_secret_with_key(value, key)?);
        }
    }
    Ok(())
}

fn encrypt_string_with_key(slot: &mut String, key: &[u8; KEY_LEN]) -> Result<()> {
    if !slot.is_empty() {
        *slot = encrypt_secret_with_key(slot, key)?;
    }
    Ok(())
}

fn decrypt_string_with_key(slot: &mut String, key: &[u8; KEY_LEN]) -> Result<()> {
    if !slot.is_empty() {
        *slot = decrypt_secret_with_key(slot, key)?;
    }
    Ok(())
}

fn is_plaintext_secret(value: &str) -> bool {
    !value.is_empty() && !is_encrypted(value)
}

/// 检查服务器配置中是否仍存在未加密的敏感字段
pub fn has_plaintext_secrets_in_servers(config: &ServerConfig) -> bool {
    config.servers.iter().any(|s| {
        s.password_encrypted
            .as_deref()
            .is_some_and(is_plaintext_secret)
            || s.key_passphrase_encrypted
                .as_deref()
                .is_some_and(is_plaintext_secret)
            || s.proxy
                .as_ref()
                .and_then(|p| p.password_encrypted.as_deref())
                .is_some_and(is_plaintext_secret)
    })
}

/// 检查应用设置中是否仍存在未加密的敏感字段
pub fn has_plaintext_secrets_in_settings(settings: &AppSettings) -> bool {
    is_plaintext_secret(&settings.connection.global_proxy_password)
        || is_plaintext_secret(&settings.sync.webdav_password)
        || settings
            .ai
            .providers
            .values()
            .any(|p| is_plaintext_secret(&p.api_key))
        || settings
            .ai
            .custom_providers
            .iter()
            .any(|p| is_plaintext_secret(&p.api_key))
}

/// 返回已加密所有敏感字段的 ServerConfig 副本（供落盘前调用）
pub fn encrypt_server_config_with_key(
    config: &ServerConfig,
    key: &[u8; KEY_LEN],
) -> Result<ServerConfig> {
    let mut encrypted = config.clone();
    for server in &mut encrypted.servers {
        encrypt_opt_with_key(&mut server.password_encrypted, key)?;
        encrypt_opt_with_key(&mut server.key_passphrase_encrypted, key)?;
        if let Some(proxy) = &mut server.proxy {
            encrypt_opt_with_key(&mut proxy.password_encrypted, key)?;
        }
    }
    Ok(encrypted)
}

/// 原地解密 ServerConfig 中的所有敏感字段（供读盘后调用）
pub fn decrypt_server_config_with_key(
    config: &mut ServerConfig,
    key: &[u8; KEY_LEN],
) -> Result<()> {
    for server in &mut config.servers {
        decrypt_opt_with_key(&mut server.password_encrypted, key)?;
        decrypt_opt_with_key(&mut server.key_passphrase_encrypted, key)?;
        if let Some(proxy) = &mut server.proxy {
            decrypt_opt_with_key(&mut proxy.password_encrypted, key)?;
        }
    }
    Ok(())
}

/// 返回已加密所有敏感字段的 AppSettings 副本（供落盘前调用）
pub fn encrypt_app_settings_with_key(
    settings: &AppSettings,
    key: &[u8; KEY_LEN],
) -> Result<AppSettings> {
    let mut encrypted = settings.clone();
    encrypt_string_with_key(&mut encrypted.connection.global_proxy_password, key)?;
    encrypt_string_with_key(&mut encrypted.sync.webdav_password, key)?;
    for provider in encrypted.ai.providers.values_mut() {
        encrypt_string_with_key(&mut provider.api_key, key)?;
    }
    for custom in &mut encrypted.ai.custom_providers {
        encrypt_string_with_key(&mut custom.api_key, key)?;
    }
    Ok(encrypted)
}

/// 原地解密 AppSettings 中的所有敏感字段（供读盘后调用）
pub fn decrypt_app_settings_with_key(
    settings: &mut AppSettings,
    key: &[u8; KEY_LEN],
) -> Result<()> {
    decrypt_string_with_key(&mut settings.connection.global_proxy_password, key)?;
    decrypt_string_with_key(&mut settings.sync.webdav_password, key)?;
    for provider in settings.ai.providers.values_mut() {
        decrypt_string_with_key(&mut provider.api_key, key)?;
    }
    for custom in &mut settings.ai.custom_providers {
        decrypt_string_with_key(&mut custom.api_key, key)?;
    }
    Ok(())
}

pub fn encrypt_server_config(config: &ServerConfig) -> Result<ServerConfig> {
    let key = master_key()?;
    encrypt_server_config_with_key(config, &key)
}

pub fn decrypt_server_config(config: &mut ServerConfig) -> Result<()> {
    let has_any_encrypted = config.servers.iter().any(|s| {
        s.password_encrypted.as_deref().is_some_and(is_encrypted)
            || s.key_passphrase_encrypted
                .as_deref()
                .is_some_and(is_encrypted)
            || s.proxy
                .as_ref()
                .and_then(|p| p.password_encrypted.as_deref())
                .is_some_and(is_encrypted)
    });
    if !has_any_encrypted {
        return Ok(());
    }
    let key = master_key()?;
    decrypt_server_config_with_key(config, &key)
}

pub fn encrypt_app_settings(settings: &AppSettings) -> Result<AppSettings> {
    let key = master_key()?;
    encrypt_app_settings_with_key(settings, &key)
}

pub fn decrypt_app_settings(settings: &mut AppSettings) -> Result<()> {
    let has_any_encrypted = is_encrypted(&settings.connection.global_proxy_password)
        || is_encrypted(&settings.sync.webdav_password)
        || settings
            .ai
            .providers
            .values()
            .any(|p| is_encrypted(&p.api_key))
        || settings
            .ai
            .custom_providers
            .iter()
            .any(|p| is_encrypted(&p.api_key));
    if !has_any_encrypted {
        return Ok(());
    }
    let key = master_key()?;
    decrypt_app_settings_with_key(settings, &key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::server::ProxyConfig;
    use crate::models::ServerData;

    #[test]
    fn secret_encrypt_decrypt_round_trip_and_legacy_plaintext_compat() {
        let key = [0x42u8; KEY_LEN];
        let encrypted = encrypt_secret_with_key("s3cr3t-pass!", &key).unwrap();
        assert!(is_encrypted(&encrypted));
        assert_ne!(encrypted, "s3cr3t-pass!");
        assert_eq!(decrypt_secret_with_key(&encrypted, &key).unwrap(), "s3cr3t-pass!");

        // 历史明文直接透传，不报错
        assert_eq!(
            decrypt_secret_with_key("legacy-plain", &key).unwrap(),
            "legacy-plain"
        );
        // 空字符串保持空字符串
        assert_eq!(encrypt_secret_with_key("", &key).unwrap(), "");
        // 已加密字符串不会二次加密
        assert_eq!(encrypt_secret_with_key(&encrypted, &key).unwrap(), encrypted);
    }

    #[test]
    fn server_config_and_settings_encrypt_all_credential_fields() {
        let key = [0x7Au8; KEY_LEN];
        let config = ServerConfig {
            groups: vec![],
            servers: vec![ServerData {
                password_encrypted: Some("ssh-pass".into()),
                key_passphrase_encrypted: Some("key-pass".into()),
                proxy: Some(ProxyConfig {
                    enabled: true,
                    password_encrypted: Some("proxy-pass".into()),
                    ..ProxyConfig::default()
                }),
                ..ServerData::default()
            }],
        };
        assert!(has_plaintext_secrets_in_servers(&config));

        let mut encrypted = encrypt_server_config_with_key(&config, &key).unwrap();
        assert!(!has_plaintext_secrets_in_servers(&encrypted));
        assert!(is_encrypted(
            encrypted.servers[0].password_encrypted.as_deref().unwrap()
        ));
        assert!(is_encrypted(
            encrypted.servers[0]
                .key_passphrase_encrypted
                .as_deref()
                .unwrap()
        ));
        assert!(is_encrypted(
            encrypted.servers[0]
                .proxy
                .as_ref()
                .unwrap()
                .password_encrypted
                .as_deref()
                .unwrap()
        ));

        decrypt_server_config_with_key(&mut encrypted, &key).unwrap();
        assert_eq!(
            encrypted.servers[0].password_encrypted.as_deref(),
            Some("ssh-pass")
        );
        assert_eq!(
            encrypted.servers[0].key_passphrase_encrypted.as_deref(),
            Some("key-pass")
        );
        assert_eq!(
            encrypted.servers[0]
                .proxy
                .as_ref()
                .unwrap()
                .password_encrypted
                .as_deref(),
            Some("proxy-pass")
        );

        let mut settings = AppSettings::default();
        settings.connection.global_proxy_password = "global-proxy".into();
        settings
            .ai
            .providers
            .get_mut("openai")
            .unwrap()
            .api_key = "sk-test-123".into();
        assert!(has_plaintext_secrets_in_settings(&settings));

        let mut enc_settings = encrypt_app_settings_with_key(&settings, &key).unwrap();
        assert!(!has_plaintext_secrets_in_settings(&enc_settings));
        assert!(is_encrypted(&enc_settings.connection.global_proxy_password));
        assert!(is_encrypted(&enc_settings.ai.providers["openai"].api_key));

        decrypt_app_settings_with_key(&mut enc_settings, &key).unwrap();
        assert_eq!(enc_settings.connection.global_proxy_password, "global-proxy");
        assert_eq!(enc_settings.ai.providers["openai"].api_key, "sk-test-123");
    }

    #[test]
    fn key_file_is_created_and_reloaded_stably() {
        let dir = std::env::temp_dir().join(format!(
            "shellmaster-crypto-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".secret.key");

        let k1 = load_or_create_key_at(&path).unwrap();
        let k2 = load_or_create_key_at(&path).unwrap();
        assert_eq!(k1, k2);
        fs::remove_dir_all(&dir).unwrap();
    }
}
