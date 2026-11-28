//! WebDAV 多端同步引擎：
//! - 3-Way 合并（基于本地 `sync_state.json` 基线识别单端修改 vs 双端并发冲突）
//! - 墓碑机制（Tombstone）传播服务器/分组/快捷命令删除操作，防止已删条目复活（90 天自动 GC）
//! - 端到端 AES-256-GCM 加密（含可选 SSH 私钥文件安全同步）
//! - WebDAV `ETag` / `If-Match` 乐观并发重试 + 进程内互斥锁
//! - 同步前本地滚动备份（保留最近 5 份）+ 变更防抖自动推送

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use rand::RngCore;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::models::settings::{AppSettings, ConflictStrategy, SyncSettings};
use crate::models::snippets::{SnippetCommand, SnippetGroup, SnippetsConfig};
use crate::models::{ServerConfig, ServerData, ServerGroupData};
use crate::services::storage;

const SYNC_BUNDLE_FILENAME: &str = "shellmaster-sync.json";
const SYNC_STATE_FILENAME: &str = "sync_state.json";
const BACKUPS_DIR_NAME: &str = "backups";
const MAX_BACKUP_SNAPSHOTS: usize = 5;
const TOMBSTONE_TTL_DAYS: i64 = 90;
const KDF_ROUNDS: usize = 100_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const AUTO_SYNC_DEBOUNCE_SECS: u64 = 8;
const POST_SYNC_COOLDOWN_MS: u64 = 15_000;
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 12;
const HTTP_SYNC_TIMEOUT_SECS: u64 = 45;
const HTTP_TEST_TIMEOUT_SECS: u64 = 20;
const HTTP_MAX_RETRIES: usize = 2;

/// 进程内同步互斥锁，防止启动自动同步、防抖推送与手动同步并发交错写盘
static SYNC_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

/// 每次同步落盘递增的数据版本号，供 UI 视图感知后台同步完成后自动刷新
static SYNC_DATA_REVISION: AtomicU64 = AtomicU64::new(0);

/// 本地数据变更防抖序号
static AUTO_SYNC_SEQ: AtomicU64 = AtomicU64::new(0);

/// 最近一次同步完成的时间戳（Unix 毫秒），用于限制自动同步与上一轮同步之间的最小冷却间隔
static LAST_SYNC_FINISHED_MS: AtomicU64 = AtomicU64::new(0);

fn now_epoch_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

pub fn data_revision() -> u64 {
    SYNC_DATA_REVISION.load(Ordering::SeqCst)
}

fn bump_data_revision() {
    SYNC_DATA_REVISION.fetch_add(1, Ordering::SeqCst);
}

/// 通知本地数据发生变化：若开启了「启用同步 + 自动同步」，经过防抖与同步后冷却窗口后自动推送到 WebDAV。
pub fn notify_local_data_changed() {
    if cfg!(test) {
        return;
    }
    let settings = storage::cached_settings();
    if !settings.sync.enabled
        || !settings.sync.auto_sync
        || settings.sync.webdav_url.trim().is_empty()
    {
        return;
    }

    let seq = AUTO_SYNC_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    crate::ssh::manager::SshManager::global()
        .runtime()
        .spawn(async move {
            tokio::time::sleep(Duration::from_secs(AUTO_SYNC_DEBOUNCE_SECS)).await;
            if AUTO_SYNC_SEQ.load(Ordering::SeqCst) != seq {
                return;
            }

            // 若刚完成过一次同步（例如刚点过「立即同步」），等待冷却窗口结束，避免背靠背连击把挂载网盘卡死
            let last_ms = LAST_SYNC_FINISHED_MS.load(Ordering::SeqCst);
            if last_ms > 0 {
                let elapsed = now_epoch_ms().saturating_sub(last_ms);
                if elapsed < POST_SYNC_COOLDOWN_MS {
                    tokio::time::sleep(Duration::from_millis(POST_SYNC_COOLDOWN_MS - elapsed))
                        .await;
                    if AUTO_SYNC_SEQ.load(Ordering::SeqCst) != seq {
                        return;
                    }
                }
            }

            if let Ok(fresh) = storage::load_settings() {
                if fresh.sync.enabled
                    && fresh.sync.auto_sync
                    && !fresh.sync.webdav_url.trim().is_empty()
                {
                    if let Err(e) = sync_webdav(&fresh.sync).await {
                        tracing::warn!("变更后自动同步 WebDAV 失败: {}", e);
                    }
                }
            }
        });
}

/// 同步结果摘要
#[derive(Clone, Debug)]
pub struct SyncReport {
    pub servers_count: usize,
    pub groups_count: usize,
    pub snippets_count: usize,
    pub keys_synced: usize,
    pub encrypted: bool,
    pub synced_at: String,
}

/// 墓碑实体类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TombstoneKind {
    Server,
    #[allow(dead_code)]
    Group,
    SnippetGroup,
    SnippetCommand,
}

/// 软删除墓碑表（id -> 删除时间戳 RFC3339）
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TombstoneSet {
    #[serde(default)]
    pub servers: HashMap<String, String>,
    #[serde(default)]
    pub groups: HashMap<String, String>,
    #[serde(default)]
    pub snippet_groups: HashMap<String, String>,
    #[serde(default)]
    pub snippet_commands: HashMap<String, String>,
}

/// 本机同步基线状态（`sync_state.json`，仅保存在本机供 3-Way 合并与删除检测使用）
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LocalSyncState {
    #[serde(default)]
    pub known_servers: HashMap<String, String>,
    #[serde(default)]
    pub known_groups: HashMap<String, String>,
    #[serde(default)]
    pub known_snippet_groups: HashMap<String, String>,
    #[serde(default)]
    pub known_snippet_commands: HashMap<String, String>,
    #[serde(default)]
    pub tombstones: TombstoneSet,
    #[serde(default)]
    pub last_synced_settings_updated_at: Option<String>,
}

/// 同步数据包内容
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SyncBundle {
    pub updated_at: String,
    #[serde(default)]
    pub servers: Option<Vec<ServerData>>,
    #[serde(default)]
    pub groups: Option<Vec<ServerGroupData>>,
    #[serde(default)]
    pub snippets: Option<SnippetsConfig>,
    #[serde(default)]
    pub settings: Option<AppSettings>,
    #[serde(default)]
    pub tombstones: TombstoneSet,
    /// SSH 私钥文件（文件名 -> Base64 原文；仅在开启 sync_keys 且设置加密口令时附带）
    #[serde(default)]
    pub private_keys: HashMap<String, String>,
}

/// 远端 WebDAV 文件封包格式（支持端到端加密与无密文纯配置两种模式）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
enum SyncEnvelope {
    Encrypted {
        version: u32,
        salt_b64: String,
        nonce_b64: String,
        ciphertext_b64: String,
    },
    Plain {
        version: u32,
        bundle: SyncBundle,
    },
}

fn sync_state_file() -> anyhow::Result<PathBuf> {
    Ok(storage::get_config_dir()?.join(SYNC_STATE_FILENAME))
}

fn load_local_sync_state() -> LocalSyncState {
    let Ok(path) = sync_state_file() else {
        return LocalSyncState::default();
    };
    if !path.exists() {
        return LocalSyncState::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_local_sync_state(state: &LocalSyncState) -> anyhow::Result<()> {
    let path = sync_state_file()?;
    let json = serde_json::to_string_pretty(state)?;
    storage::write_private_file(&path, json.as_bytes())?;
    Ok(())
}

fn tombstone_map_mut(set: &mut TombstoneSet, kind: TombstoneKind) -> &mut HashMap<String, String> {
    match kind {
        TombstoneKind::Server => &mut set.servers,
        TombstoneKind::Group => &mut set.groups,
        TombstoneKind::SnippetGroup => &mut set.snippet_groups,
        TombstoneKind::SnippetCommand => &mut set.snippet_commands,
    }
}

/// 记录本机删除操作为墓碑，防止下次同步时从另一台设备复活
pub fn record_local_deletion(kind: TombstoneKind, id: &str) {
    if cfg!(test) || id.trim().is_empty() {
        return;
    }
    let mut state = load_local_sync_state();
    let now = Utc::now().to_rfc3339();
    tombstone_map_mut(&mut state.tombstones, kind).insert(id.to_string(), now);
    let _ = save_local_sync_state(&state);
}

/// 批量记录本机删除操作为墓碑（例如级联删除快捷命令组）
pub fn record_local_deletions(kind: TombstoneKind, ids: &[String]) {
    if cfg!(test) || ids.is_empty() {
        return;
    }
    let mut state = load_local_sync_state();
    let now = Utc::now().to_rfc3339();
    let map = tombstone_map_mut(&mut state.tombstones, kind);
    for id in ids {
        if !id.trim().is_empty() {
            map.insert(id.clone(), now.clone());
        }
    }
    let _ = save_local_sync_state(&state);
}

/// 若同 ID 条目在本机被重新添加，清除其旧墓碑
pub fn record_entity_alive(kind: TombstoneKind, id: &str) {
    if cfg!(test) || id.trim().is_empty() {
        return;
    }
    let mut state = load_local_sync_state();
    if tombstone_map_mut(&mut state.tombstones, kind)
        .remove(id)
        .is_some()
    {
        let _ = save_local_sync_state(&state);
    }
}

fn build_client(timeout_secs: u64) -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(timeout_secs))
        .pool_idle_timeout(Duration::from_secs(15))
        .user_agent(concat!("ShellMaster/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))
}

/// 提取服务端错误响应正文的可读摘要（剔除 HTML 标签与多余空白，最多保留 160 字符）
pub(crate) fn summarize_http_body(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut plain = String::with_capacity(trimmed.len().min(256));
    let mut in_tag = false;
    for ch in trimmed.chars() {
        match ch {
            '<' => {
                in_tag = true;
                plain.push(' ');
            }
            '>' => {
                in_tag = false;
            }
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(160).collect()
}

fn format_http_error(prefix: &str, status: u16, body: &str) -> String {
    let snippet = summarize_http_body(body);
    if snippet.is_empty() {
        format!("{prefix} (HTTP {status})")
    } else {
        format!("{prefix} (HTTP {status}): {snippet}")
    }
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

fn is_retryable_reqwest_err(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

fn apply_auth(req: reqwest::RequestBuilder, cfg: &SyncSettings) -> reqwest::RequestBuilder {
    let user = cfg.webdav_username.trim();
    if user.is_empty() && cfg.webdav_password.is_empty() {
        req
    } else {
        req.basic_auth(user, Some(&cfg.webdav_password))
    }
}

/// 规范化 WebDAV 基础 URL 与存储子路径，返回目录 URL（不含末尾 `/`）
pub fn join_webdav_url(base_url: &str, remote_path: &str) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("WebDAV 服务器地址为空".to_string());
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("WebDAV 服务器地址必须以 http:// 或 https:// 开头".to_string());
    }
    let rel = remote_path.trim().trim_matches('/');
    if rel.is_empty() {
        Ok(base.to_string())
    } else {
        Ok(format!("{base}/{rel}"))
    }
}

/// 构造同步数据包完整远端 URL
pub fn remote_bundle_url(base_url: &str, remote_path: &str) -> Result<String, String> {
    let dir = join_webdav_url(base_url, remote_path)?;
    Ok(format!("{dir}/{SYNC_BUNDLE_FILENAME}"))
}

/// 校验 SSH 私钥文件名，彻底阻断路径穿越、隐藏文件与目录分隔符
pub(crate) fn safe_key_filename(name: &str) -> Option<String> {
    if name.is_empty()
        || name != name.trim()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        return None;
    }
    let mut comps = Path::new(name).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Some(name.to_string()),
        _ => None,
    }
}

/// 测试 WebDAV 连通性与认证凭据
///
/// 1. 优先向目标路径发送 `PROPFIND (Depth: 0)`；
/// 2. `200 / 207` 直接通过；`401 / 403` 返回认证失败提示；
/// 3. 目标子目录 `404`（首次使用尚未创建目录）时，回退探测基础 URL（`PROPFIND` / `OPTIONS`），
///    若基础地址可达则返回通过并提示首次同步将自动建目录。
pub async fn test_webdav_connection(cfg: &SyncSettings) -> Result<String, String> {
    let base = cfg.webdav_url.trim().trim_end_matches('/');
    let dir_url = join_webdav_url(base, &cfg.webdav_path)?;
    let client = build_client(HTTP_TEST_TIMEOUT_SECS)?;
    let propfind = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;

    let resp = apply_auth(
        client
            .request(propfind.clone(), &dir_url)
            .header("Depth", "0"),
        cfg,
    )
    .send()
    .await
    .map_err(|e| format!("无法连接 WebDAV 服务器: {e}"))?;

    let status = resp.status().as_u16();
    match status {
        200 | 204 | 207 => Ok("WebDAV 连接成功，目标目录可访问".to_string()),
        401 | 403 => Err(format!(
            "认证失败 (HTTP {status})，请检查用户名或应用专用密码"
        )),
        404 => {
            // 目标子目录尚未创建：探测根目录是否可达且凭据正确
            let root_resp = apply_auth(
                client.request(propfind, base).header("Depth", "0"),
                cfg,
            )
            .send()
            .await
            .map_err(|e| format!("无法连接 WebDAV 根目录: {e}"))?;
            let root_status = root_resp.status().as_u16();
            match root_status {
                200 | 204 | 207 => {
                    Ok("WebDAV 连接成功（首次同步时将自动创建存储目录）".to_string())
                }
                401 | 403 => Err(format!(
                    "认证失败 (HTTP {root_status})，请检查用户名或应用专用密码"
                )),
                405 => {
                    // 部分 WebDAV 网关对根目录禁用 PROPFIND，尝试 OPTIONS
                    let opt_resp = apply_auth(client.request(Method::OPTIONS, base), cfg)
                        .send()
                        .await
                        .map_err(|e| format!("OPTIONS 探测失败: {e}"))?;
                    let opt_status = opt_resp.status().as_u16();
                    if (200..300).contains(&opt_status) {
                        Ok("WebDAV 连接成功（首次同步时将自动创建存储目录）".to_string())
                    } else {
                        Err(format!("WebDAV 服务端返回异常状态码 (HTTP {opt_status})"))
                    }
                }
                _ => Err(format!(
                    "WebDAV 根目录不可访问 (HTTP {root_status})，请检查服务器地址"
                )),
            }
        }
        _ => {
            let body = resp.text().await.unwrap_or_default();
            Err(format_http_error("WebDAV 服务端返回错误", status, &body))
        }
    }
}

/// 按需确保远端 WebDAV 存储目录存在：
/// 仅在远端同步文件不存在（首次同步或目录被删）时调用；
/// 从最深层目标目录往回通过 `PROPFIND (Depth: 0)` 查找首个已存在的父级，仅对缺失的尾部子目录执行 `MKCOL`，
/// 避免每次同步都向 CloudDrive2 / Alist 的顶层虚拟挂载根重复发送 `MKCOL`。
async fn ensure_remote_dir_lazy(client: &Client, cfg: &SyncSettings) -> Result<(), String> {
    let base = cfg.webdav_url.trim().trim_end_matches('/');
    let rel = cfg.webdav_path.trim().trim_matches('/');
    if rel.is_empty() {
        return Ok(());
    }

    let mut dirs: Vec<String> = Vec::new();
    let mut acc = base.to_string();
    for segment in rel.split('/').filter(|s| !s.is_empty()) {
        acc.push('/');
        acc.push_str(segment);
        dirs.push(acc.clone());
    }
    if dirs.is_empty() {
        return Ok(());
    }

    let propfind = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
    let mkcol = Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;

    let mut create_from = 0usize;
    for idx in (0..dirs.len()).rev() {
        let resp = apply_auth(
            client
                .request(propfind.clone(), &dirs[idx])
                .header("Depth", "0"),
            cfg,
        )
        .send()
        .await;

        match resp {
            Ok(r) => {
                let code = r.status().as_u16();
                if matches!(code, 200 | 204 | 207) {
                    create_from = idx + 1;
                    break;
                }
                if matches!(code, 401 | 403) {
                    return Err(format!(
                        "WebDAV 目录访问认证失败 (HTTP {code})，请检查用户名或密码"
                    ));
                }
                if code == 404 {
                    continue;
                }
                // 网关对目录禁用 PROPFIND 时停止继续向顶层回溯
                create_from = idx;
                break;
            }
            Err(_) => {
                create_from = idx;
                break;
            }
        }
    }

    for dir_url in &dirs[create_from..] {
        let resp = apply_auth(client.request(mkcol.clone(), dir_url), cfg)
            .send()
            .await
            .map_err(|e| format!("创建远端目录失败 ({dir_url}): {e}"))?;
        let code = resp.status().as_u16();
        // 201 创建成功，405/301/409/200/204 通常表示目录已存在
        if matches!(code, 200 | 201 | 204 | 301 | 405 | 409) {
            continue;
        }
        if matches!(code, 401 | 403) {
            return Err(format!("无权创建远端目录 (HTTP {code})，请检查 WebDAV 权限"));
        }
        let body = resp.text().await.unwrap_or_default();
        return Err(format_http_error(
            &format!("创建远端目录失败 ({dir_url})"),
            code,
            &body,
        ));
    }
    Ok(())
}

/// 由用户同步口令与随机盐派生 256-bit 加密密钥（100,000 轮迭代 SHA-256）
fn derive_sync_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(passphrase.as_bytes());
    let mut digest: [u8; 32] = hasher.finalize().into();

    for _ in 1..KDF_ROUNDS {
        let mut round = Sha256::new();
        round.update(digest);
        round.update(salt);
        round.update(passphrase.as_bytes());
        digest = round.finalize().into();
    }
    digest
}

/// 将同步包封包：设置了同步口令时端到端 AES-256-GCM 加密；未设置口令时剥离所有敏感凭据与私钥
fn seal_bundle(bundle: &SyncBundle, passphrase: &str) -> Result<String, String> {
    let pass = passphrase.trim();
    let envelope = if !pass.is_empty() {
        let mut salt = [0u8; SALT_LEN];
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let key = derive_sync_key(pass, &salt);
        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|e| format!("初始化加密器失败: {e}"))?;
        let plaintext =
            serde_json::to_vec(bundle).map_err(|e| format!("序列化同步数据失败: {e}"))?;
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
            .map_err(|e| format!("加密同步数据失败: {e}"))?;

        SyncEnvelope::Encrypted {
            version: 1,
            salt_b64: BASE64.encode(salt),
            nonce_b64: BASE64.encode(nonce_bytes),
            ciphertext_b64: BASE64.encode(ciphertext),
        }
    } else {
        let mut sanitized = bundle.clone();
        strip_bundle_secrets(&mut sanitized);
        SyncEnvelope::Plain {
            version: 1,
            bundle: sanitized,
        }
    };

    serde_json::to_string_pretty(&envelope).map_err(|e| format!("序列化封包失败: {e}"))
}

/// 解包远端同步文件（支持端到端解密与无密文模式）
fn open_bundle(raw: &str, passphrase: &str) -> Result<(SyncBundle, bool), String> {
    let envelope: SyncEnvelope =
        serde_json::from_str(raw).map_err(|e| format!("远端同步文件格式无效: {e}"))?;
    match envelope {
        SyncEnvelope::Encrypted {
            salt_b64,
            nonce_b64,
            ciphertext_b64,
            ..
        } => {
            let pass = passphrase.trim();
            if pass.is_empty() {
                return Err("远端同步包已加密，请先填写「同步加密口令」".to_string());
            }
            let salt = BASE64
                .decode(salt_b64.as_bytes())
                .map_err(|_| "远端同步包 salt 无效".to_string())?;
            let nonce = BASE64
                .decode(nonce_b64.as_bytes())
                .map_err(|_| "远端同步包 nonce 无效".to_string())?;
            let ciphertext = BASE64
                .decode(ciphertext_b64.as_bytes())
                .map_err(|_| "远端同步包密文无效".to_string())?;
            if nonce.len() != NONCE_LEN {
                return Err("远端同步包 nonce 长度无效".to_string());
            }
            let key = derive_sync_key(pass, &salt);
            let cipher =
                Aes256Gcm::new_from_slice(&key).map_err(|e| format!("初始化解密器失败: {e}"))?;
            let plaintext = cipher
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                .map_err(|_| "同步加密口令不正确，无法解密远端配置".to_string())?;
            let bundle: SyncBundle = serde_json::from_slice(&plaintext)
                .map_err(|e| format!("解密后数据解析失败: {e}"))?;
            Ok((bundle, true))
        }
        SyncEnvelope::Plain { bundle, .. } => Ok((bundle, false)),
    }
}

/// 未设置加密口令时剥离所有敏感凭据与私钥文件，避免明文密码、API Key 或私钥上传到远端 WebDAV
fn strip_bundle_secrets(bundle: &mut SyncBundle) {
    bundle.private_keys.clear();
    if let Some(servers) = &mut bundle.servers {
        for s in servers {
            s.password_encrypted = None;
            s.key_passphrase_encrypted = None;
            if let Some(proxy) = &mut s.proxy {
                proxy.password_encrypted = None;
            }
        }
    }
    if let Some(settings) = &mut bundle.settings {
        settings.connection.global_proxy_password.clear();
        settings.sync.webdav_password.clear();
        settings.sync.sync_passphrase.clear();
        for p in settings.ai.providers.values_mut() {
            p.api_key.clear();
        }
        for c in &mut settings.ai.custom_providers {
            c.api_key.clear();
        }
    }
}

/// 统一实体抽象，用于执行 3-Way 合并与墓碑传播
pub(crate) trait SyncEntity: Clone + Serialize {
    fn entity_id(&self) -> &str;
    fn updated_at(&self) -> Option<&str>;
    fn created_at(&self) -> Option<&str>;

    fn entity_ts(&self) -> &str {
        self.updated_at()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| self.created_at().map(str::trim).filter(|s| !s.is_empty()))
            .unwrap_or("")
    }

    fn version_token(&self) -> String {
        let ts = self.entity_ts();
        if !ts.is_empty() {
            return ts.to_string();
        }
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let digest = Sha256::digest(&bytes);
        BASE64.encode(digest)
    }
}

impl SyncEntity for ServerData {
    fn entity_id(&self) -> &str {
        &self.id
    }
    fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }
    fn created_at(&self) -> Option<&str> {
        Some(&self.created_at)
    }
}

impl SyncEntity for ServerGroupData {
    fn entity_id(&self) -> &str {
        &self.id
    }
    fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }
    fn created_at(&self) -> Option<&str> {
        None
    }
}

impl SyncEntity for SnippetGroup {
    fn entity_id(&self) -> &str {
        &self.id
    }
    fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }
    fn created_at(&self) -> Option<&str> {
        Some(&self.created_at)
    }
}

impl SyncEntity for SnippetCommand {
    fn entity_id(&self) -> &str {
        &self.id
    }
    fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }
    fn created_at(&self) -> Option<&str> {
        Some(&self.created_at)
    }
}

fn detect_implicit_deletions<T: SyncEntity>(
    known: &HashMap<String, String>,
    local: &[T],
    tombstones: &mut HashMap<String, String>,
    now: &str,
) {
    let current_ids: HashSet<&str> = local.iter().map(|item| item.entity_id()).collect();
    for id in known.keys() {
        if !current_ids.contains(id.as_str()) {
            tombstones
                .entry(id.clone())
                .or_insert_with(|| now.to_string());
        }
    }
}

fn merge_tombstone_maps(
    local: &mut HashMap<String, String>,
    remote: &HashMap<String, String>,
) {
    for (id, r_ts) in remote {
        match local.get_mut(id) {
            Some(l_ts) => {
                if *r_ts > *l_ts {
                    *l_ts = r_ts.clone();
                }
            }
            None => {
                local.insert(id.clone(), r_ts.clone());
            }
        }
    }
}

fn prune_tombstone_map(map: &mut HashMap<String, String>, cutoff: DateTime<Utc>) {
    map.retain(|_, ts| {
        DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt.with_timezone(&Utc) >= cutoff)
            .unwrap_or(false)
    });
}

fn prune_expired_tombstones(set: &mut TombstoneSet, now: DateTime<Utc>) {
    let cutoff = now - chrono::Duration::days(TOMBSTONE_TTL_DAYS);
    prune_tombstone_map(&mut set.servers, cutoff);
    prune_tombstone_map(&mut set.groups, cutoff);
    prune_tombstone_map(&mut set.snippet_groups, cutoff);
    prune_tombstone_map(&mut set.snippet_commands, cutoff);
}

fn is_newer_than_tombstone(entity_ts: &str, del_ts: &str) -> bool {
    if entity_ts.is_empty() {
        return false;
    }
    match (
        DateTime::parse_from_rfc3339(entity_ts),
        DateTime::parse_from_rfc3339(del_ts),
    ) {
        (Ok(et), Ok(dt)) => et > dt,
        _ => entity_ts > del_ts,
    }
}

/// 通用 3-Way 实体合并算法（支持软删除墓碑传播、单边变更无冲突合入、以及冲突策略仲裁）
pub(crate) fn merge_entities_3way<T: SyncEntity, F>(
    local: Vec<T>,
    remote: Vec<T>,
    known: &HashMap<String, String>,
    local_tombstones: &mut HashMap<String, String>,
    remote_tombstones: &HashMap<String, String>,
    strategy: ConflictStrategy,
    on_pair: F,
) -> Vec<T>
where
    F: Fn(T, T) -> T,
{
    merge_tombstone_maps(local_tombstones, remote_tombstones);

    let mut ordered_ids: Vec<String> = Vec::new();
    let mut local_map: HashMap<String, T> = HashMap::new();
    for item in local {
        let id = item.entity_id().to_string();
        if !local_map.contains_key(&id) {
            ordered_ids.push(id.clone());
            local_map.insert(id, item);
        }
    }

    let mut remote_map: HashMap<String, T> = HashMap::new();
    for item in remote {
        let id = item.entity_id().to_string();
        if !local_map.contains_key(&id) && !remote_map.contains_key(&id) {
            ordered_ids.push(id.clone());
        }
        remote_map.entry(id).or_insert(item);
    }

    let mut merged = Vec::with_capacity(ordered_ids.len());
    for id in ordered_ids {
        let mut l_opt = local_map.remove(&id);
        let mut r_opt = remote_map.remove(&id);

        if let Some(del_ts) = local_tombstones.get(&id).cloned() {
            let l_alive = l_opt
                .as_ref()
                .is_some_and(|l| is_newer_than_tombstone(l.entity_ts(), &del_ts));
            let r_alive = r_opt
                .as_ref()
                .is_some_and(|r| is_newer_than_tombstone(r.entity_ts(), &del_ts));
            if !l_alive && !r_alive {
                // 两端都没有比删除时间戳更新的修改：确认删除并保持墓碑
                continue;
            }
            if !l_alive {
                l_opt = None;
            }
            if !r_alive {
                r_opt = None;
            }
            // 删除后有重新编辑/重建：解除墓碑
            local_tombstones.remove(&id);
        }

        match (l_opt, r_opt) {
            (Some(l), Some(r)) => {
                let l_tok = l.version_token();
                let r_tok = r.version_token();
                let chosen = if l_tok == r_tok {
                    match strategy {
                        ConflictStrategy::RemoteFirst => on_pair(r, l),
                        _ => on_pair(l, r),
                    }
                } else {
                    let known_tok = known.get(&id);
                    let local_changed = known_tok.map(|k| k != &l_tok).unwrap_or(true);
                    let remote_changed = known_tok.map(|k| k != &r_tok).unwrap_or(true);
                    if remote_changed && !local_changed {
                        on_pair(r, l)
                    } else if local_changed && !remote_changed {
                        on_pair(l, r)
                    } else {
                        match strategy {
                            ConflictStrategy::LocalFirst => on_pair(l, r),
                            ConflictStrategy::RemoteFirst => on_pair(r, l),
                            ConflictStrategy::Merge | ConflictStrategy::Ask => {
                                if r.entity_ts() > l.entity_ts() {
                                    on_pair(r, l)
                                } else {
                                    on_pair(l, r)
                                }
                            }
                        }
                    }
                };
                merged.push(chosen);
            }
            (Some(l), None) => merged.push(l),
            (None, Some(r)) => merged.push(r),
            (None, None) => {}
        }
    }

    merged
}

fn max_optional_ts(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(sa), Some(sb)) => Some(if sb > sa { sb } else { sa }),
        (Some(sa), None) => Some(sa),
        (None, Some(sb)) => Some(sb),
        (None, None) => None,
    }
}

/// 合并同一台服务器的胜出版本与另一侧信息：保留凭据兜底与运行期元数据（OS / 最近连接时间）
fn merge_server_pair(mut winner: ServerData, loser: ServerData) -> ServerData {
    if winner.auth_type == loser.auth_type {
        if winner.password_encrypted.is_none() {
            winner.password_encrypted = loser.password_encrypted;
        }
        if winner.key_passphrase_encrypted.is_none() {
            winner.key_passphrase_encrypted = loser.key_passphrase_encrypted;
        }
    }
    if let (Some(wp), Some(lp)) = (&mut winner.proxy, &loser.proxy) {
        if wp.password_encrypted.is_none() {
            wp.password_encrypted = lp.password_encrypted.clone();
        }
    }
    if winner.os_name.is_none() {
        winner.os_name = loser.os_name;
    }
    winner.last_connected_at = max_optional_ts(winner.last_connected_at, loser.last_connected_at);
    winner
}

/// 合并后修正服务器与分组的一致性：补齐存活服务器引用的分组并清理空分组
fn reconcile_server_groups(
    servers: &[ServerData],
    groups: &mut Vec<ServerGroupData>,
    prune_empty: bool,
    now: &str,
) {
    for server in servers {
        if let Some(gid) = server.group_id.as_deref() {
            if !groups.iter().any(|g| g.id == gid) {
                groups.push(ServerGroupData {
                    id: gid.to_string(),
                    name: gid.to_string(),
                    icon_path: "icons/server.svg".to_string(),
                    updated_at: Some(now.to_string()),
                });
            }
        }
    }
    if prune_empty {
        let used: HashSet<&str> = servers
            .iter()
            .filter_map(|s| s.group_id.as_deref())
            .collect();
        groups.retain(|g| used.contains(g.id.as_str()));
    }
}

/// 3-Way 合并应用设置：
/// - 使用 `AppSettings.updated_at` 与 `last_synced_settings_updated_at` 单独判定设置是否冲突；
/// - 始终保留本机 WebDAV 配置与跨平台不兼容路径（`local_default_path`、`external_editor_path`）；
/// - 保留非空本机/远端 AI 密钥兜底，防止无口令设备覆盖已配置密钥。
pub(crate) fn merge_settings_3way(
    local: &AppSettings,
    remote: &AppSettings,
    last_synced_settings_at: Option<&str>,
    strategy: ConflictStrategy,
) -> AppSettings {
    let local_ts = local.updated_at.as_deref().unwrap_or("").trim();
    let remote_ts = remote.updated_at.as_deref().unwrap_or("").trim();

    let local_changed = !local_ts.is_empty()
        && last_synced_settings_at
            .map(|k| k != local_ts)
            .unwrap_or(true);
    let remote_changed = !remote_ts.is_empty()
        && last_synced_settings_at
            .map(|k| k != remote_ts)
            .unwrap_or(true);

    let use_remote = if remote_changed && !local_changed {
        true
    } else if local_changed && !remote_changed {
        false
    } else if local_ts.is_empty() && remote_ts.is_empty() {
        matches!(strategy, ConflictStrategy::RemoteFirst)
    } else {
        match strategy {
            ConflictStrategy::LocalFirst => false,
            ConflictStrategy::RemoteFirst => true,
            ConflictStrategy::Merge | ConflictStrategy::Ask => remote_ts > local_ts,
        }
    };

    let (mut winner, loser) = if use_remote {
        (remote.clone(), local)
    } else {
        (local.clone(), remote)
    };

    // WebDAV 凭据、同步策略与本机目录路径永远以当前设备为准
    winner.sync = local.sync.clone();
    winner.sftp.local_default_path = local.sftp.local_default_path.clone();
    winner.sftp.external_editor_path = local.sftp.external_editor_path.clone();

    if winner.connection.global_proxy_password.is_empty() {
        winner.connection.global_proxy_password = loser.connection.global_proxy_password.clone();
    }
    for (k, loser_p) in &loser.ai.providers {
        if let Some(wp) = winner.ai.providers.get_mut(k) {
            if wp.api_key.is_empty() && !loser_p.api_key.is_empty() {
                wp.api_key = loser_p.api_key.clone();
                wp.verified = loser_p.verified;
            }
        } else {
            winner.ai.providers.insert(k.clone(), loser_p.clone());
        }
    }
    for loser_c in &loser.ai.custom_providers {
        if let Some(wc) = winner
            .ai
            .custom_providers
            .iter_mut()
            .find(|c| c.id == loser_c.id)
        {
            if wc.api_key.is_empty() && !loser_c.api_key.is_empty() {
                wc.api_key = loser_c.api_key.clone();
                wc.verified = loser_c.verified;
            }
        } else {
            winner.ai.custom_providers.push(loser_c.clone());
        }
    }

    winner
}

/// 同步前本地快照备份，防止误覆盖（保留最近 MAX_BACKUP_SNAPSHOTS 份）
fn create_local_backup() -> anyhow::Result<()> {
    let config_dir = storage::get_config_dir()?;
    let backups_root = config_dir.join(BACKUPS_DIR_NAME);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let snapshot_dir = backups_root.join(&stamp);
    fs::create_dir_all(&snapshot_dir)?;

    let files = ["servers.json", "snippets.json", "settings.json"];
    let mut copied_any = false;
    for name in files {
        let src = config_dir.join(name);
        if src.exists() {
            if let Ok(bytes) = fs::read(&src) {
                storage::write_private_file(&snapshot_dir.join(name), &bytes)?;
                copied_any = true;
            }
        }
    }
    if !copied_any {
        let _ = fs::remove_dir_all(&snapshot_dir);
        return Ok(());
    }

    let mut dirs: Vec<PathBuf> = fs::read_dir(&backups_root)?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    while dirs.len() > MAX_BACKUP_SNAPSHOTS {
        let oldest = dirs.remove(0);
        let _ = fs::remove_dir_all(oldest);
    }
    Ok(())
}

fn snapshot_known_map<T: SyncEntity>(items: &[T]) -> HashMap<String, String> {
    items
        .iter()
        .map(|i| (i.entity_id().to_string(), i.version_token()))
        .collect()
}

fn is_strong_etag(etag: Option<&str>) -> Option<&str> {
    let raw = etag?.trim();
    if raw.is_empty() || raw.starts_with("W/") || raw.starts_with("w/") {
        None
    } else {
        Some(raw)
    }
}

/// 拉取远端同步包及强 ETag：
/// 对 CloudDrive2 / Alist 等挂载网盘常见的瞬时 5xx / 429 或网络抖动自动退避重试，并保留服务端错误详情。
async fn fetch_remote_bundle_with_retry(
    client: &Client,
    bundle_url: &str,
    cfg: &SyncSettings,
) -> Result<(Option<SyncBundle>, Option<String>), String> {
    let mut last_err = String::new();
    for retry in 0..=HTTP_MAX_RETRIES {
        if retry > 0 {
            let delay_ms = 1500u64 * (1u64 << (retry - 1));
            tracing::warn!(
                "WebDAV GET 遇到瞬时异常 ({})，{}ms 后执行第 {} 次重试...",
                last_err,
                delay_ms,
                retry
            );
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let resp = match apply_auth(client.get(bundle_url), cfg).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = if e.is_timeout() {
                    format!("拉取远端同步文件超时（挂载网盘回源响应过慢）: {e}")
                } else {
                    format!("拉取远端同步文件失败: {e}")
                };
                if retry < HTTP_MAX_RETRIES && is_retryable_reqwest_err(&e) {
                    continue;
                }
                return Err(last_err);
            }
        };

        let status = resp.status().as_u16();
        let remote_etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        match status {
            200 => match resp.text().await {
                Ok(text) => {
                    if text.trim().is_empty() {
                        return Ok((None, remote_etag));
                    }
                    let (bundle, _) = open_bundle(&text, &cfg.sync_passphrase)?;
                    return Ok((Some(bundle), remote_etag));
                }
                Err(e) => {
                    last_err = format!("读取远端同步文件流失败: {e}");
                    if retry < HTTP_MAX_RETRIES {
                        continue;
                    }
                    return Err(last_err);
                }
            },
            404 => return Ok((None, None)),
            401 | 403 => {
                return Err(format!(
                    "WebDAV 认证失败 (HTTP {status})，请检查用户名或密码"
                ));
            }
            code if is_retryable_status(code) && retry < HTTP_MAX_RETRIES => {
                let body = resp.text().await.unwrap_or_default();
                last_err = format_http_error("读取远端同步文件失败", code, &body);
                continue;
            }
            code => {
                let body = resp.text().await.unwrap_or_default();
                return Err(format_http_error("读取远端同步文件失败", code, &body));
            }
        }
    }
    Err(last_err)
}

/// 发送 PUT 请求：
/// - 若携带强 ETag，优先尝试 `If-Match`；若挂载网盘网关不兼容 `If-Match`（返回 400/405/409/500/501），自动降级为无条件 PUT；
/// - 若无条件 PUT 遇到瞬时 5xx / 429 或网络抖动，自动退避重试；
/// - 若返回 `412 Precondition Failed` 则由外层重新拉取并执行 3-Way 合并。
async fn put_bundle_with_etag(
    client: &Client,
    url: &str,
    cfg: &SyncSettings,
    payload: &str,
    etag: Option<&str>,
) -> Result<(u16, String), String> {
    if let Some(strong) = is_strong_etag(etag) {
        let conditional_resp = apply_auth(
            client
                .put(url)
                .header("Content-Type", "application/json; charset=utf-8")
                .header("If-Match", strong)
                .body(payload.to_string()),
            cfg,
        )
        .send()
        .await;

        match conditional_resp {
            Ok(resp) => {
                let code = resp.status().as_u16();
                if (200..300).contains(&code) || code == 412 {
                    return Ok((code, String::new()));
                }
                if matches!(code, 401 | 403) {
                    let body = resp.text().await.unwrap_or_default();
                    return Ok((code, body));
                }
                tracing::info!(
                    "WebDAV 带 If-Match 写入返回 HTTP {}，自动降级为无条件 PUT...",
                    code
                );
            }
            Err(e) => {
                tracing::warn!(
                    "WebDAV 带 If-Match 写入网络异常 ({})，自动降级为无条件 PUT...",
                    e
                );
            }
        }
    }

    let mut last_err = String::new();
    for retry in 0..=HTTP_MAX_RETRIES {
        if retry > 0 {
            let delay_ms = 1500u64 * (1u64 << (retry - 1));
            tracing::warn!(
                "WebDAV PUT 遇到瞬时异常 ({})，{}ms 后执行第 {} 次重试...",
                last_err,
                delay_ms,
                retry
            );
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let resp = match apply_auth(
            client
                .put(url)
                .header("Content-Type", "application/json; charset=utf-8")
                .body(payload.to_string()),
            cfg,
        )
        .send()
        .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = if e.is_timeout() {
                    format!("上传同步文件超时: {e}")
                } else {
                    format!("上传同步文件失败: {e}")
                };
                if retry < HTTP_MAX_RETRIES && is_retryable_reqwest_err(&e) {
                    continue;
                }
                return Err(last_err);
            }
        };

        let code = resp.status().as_u16();
        if (200..300).contains(&code) {
            return Ok((code, String::new()));
        }
        let body = resp.text().await.unwrap_or_default();
        if is_retryable_status(code) && retry < HTTP_MAX_RETRIES {
            last_err = format_http_error("上传同步文件失败", code, &body);
            continue;
        }
        return Ok((code, body));
    }
    Err(last_err)
}

/// 执行 WebDAV 3-Way 加密双向同步：
/// 1. 进程内互斥加锁，并取消早于本轮读盘的待执行防抖任务；
/// 2. 直接读取远端 `shellmaster-sync.json` 与强 `ETag`（文件已存在时跳过目录探测，省去多余 `MKCOL`）；
/// 3. 基于 `sync_state.json` 执行 3-Way 合并与墓碑删除传播；
/// 4. 仅在远端尚无同步文件时按需懒创建缺失子目录，随后上传合并包（支持 `412` 重试与非标网关降级）；
/// 5. 上传完成后在本地创建备份、持久化合并后配置与同步基线，并记录同步完成冷却时间戳。
pub async fn sync_webdav(cfg: &SyncSettings) -> Result<SyncReport, String> {
    let _guard = SYNC_LOCK.lock().await;
    // 本轮即将读取磁盘最新快照，作废此前排队的防抖自动同步任务
    AUTO_SYNC_SEQ.fetch_add(1, Ordering::SeqCst);

    let bundle_url = remote_bundle_url(&cfg.webdav_url, &cfg.webdav_path)?;
    let client = build_client(HTTP_SYNC_TIMEOUT_SECS)?;

    let mut local_servers_disk = storage::load_servers().unwrap_or(ServerConfig {
        groups: Vec::new(),
        servers: Vec::new(),
    });
    let local_snippets_disk = storage::load_snippets().unwrap_or_default();
    let local_settings_disk = storage::load_settings().unwrap_or_default();
    let initial_sync_state = load_local_sync_state();
    let encrypted = !cfg.sync_passphrase.trim().is_empty();

    // 保留合并过程中使用的本地快照（若遇到 412 需要基于原始本地数据重新合并）
    let mut final_servers = local_servers_disk.clone();
    let mut final_snippets = local_snippets_disk.clone();
    let mut final_settings = local_settings_disk.clone();
    final_settings.sync = cfg.clone();
    let mut final_sync_state = initial_sync_state.clone();
    let mut final_remote_keys: HashMap<String, String> = HashMap::new();
    let mut final_upload_keys: HashMap<String, String> = HashMap::new();
    let mut had_remote = false;
    let mut dir_ensured = false;
    let mut now = Utc::now().to_rfc3339();

    for attempt in 0..2 {
        // 1. 拉取远端同步包及 ETag（自动退避重试瞬时 5xx/超时）
        let (remote_bundle, remote_etag) =
            fetch_remote_bundle_with_retry(&client, &bundle_url, cfg).await?;

        had_remote = remote_bundle.is_some();
        let now_dt = Utc::now();
        now = now_dt.to_rfc3339();

        let mut servers_work = local_servers_disk.clone();
        let mut snippets_work = local_snippets_disk.clone();
        let mut settings_work = local_settings_disk.clone();
        settings_work.sync = cfg.clone();
        let mut sync_state = initial_sync_state.clone();

        // 检测上次同步后在本地被删除的条目（兜底非经 storage API 删除的场景）
        detect_implicit_deletions(
            &sync_state.known_servers,
            &servers_work.servers,
            &mut sync_state.tombstones.servers,
            &now,
        );
        detect_implicit_deletions(
            &sync_state.known_groups,
            &servers_work.groups,
            &mut sync_state.tombstones.groups,
            &now,
        );
        detect_implicit_deletions(
            &sync_state.known_snippet_groups,
            &snippets_work.groups,
            &mut sync_state.tombstones.snippet_groups,
            &now,
        );
        detect_implicit_deletions(
            &sync_state.known_snippet_commands,
            &snippets_work.commands,
            &mut sync_state.tombstones.snippet_commands,
            &now,
        );

        final_remote_keys.clear();
        if let Some(remote) = remote_bundle {
            if cfg.sync_servers {
                if let Some(rs) = remote.servers {
                    servers_work.servers = merge_entities_3way(
                        servers_work.servers,
                        rs,
                        &sync_state.known_servers,
                        &mut sync_state.tombstones.servers,
                        &remote.tombstones.servers,
                        cfg.conflict_strategy,
                        merge_server_pair,
                    );
                } else {
                    merge_tombstone_maps(
                        &mut sync_state.tombstones.servers,
                        &remote.tombstones.servers,
                    );
                }
            }

            if cfg.sync_groups {
                if let Some(rg) = remote.groups {
                    servers_work.groups = merge_entities_3way(
                        servers_work.groups,
                        rg,
                        &sync_state.known_groups,
                        &mut sync_state.tombstones.groups,
                        &remote.tombstones.groups,
                        cfg.conflict_strategy,
                        |winner, _| winner,
                    );
                } else {
                    merge_tombstone_maps(
                        &mut sync_state.tombstones.groups,
                        &remote.tombstones.groups,
                    );
                }
            }

            if cfg.sync_snippets {
                if let Some(rsnip) = remote.snippets {
                    snippets_work.groups = merge_entities_3way(
                        snippets_work.groups,
                        rsnip.groups,
                        &sync_state.known_snippet_groups,
                        &mut sync_state.tombstones.snippet_groups,
                        &remote.tombstones.snippet_groups,
                        cfg.conflict_strategy,
                        |winner, _| winner,
                    );
                    snippets_work.commands = merge_entities_3way(
                        snippets_work.commands,
                        rsnip.commands,
                        &sync_state.known_snippet_commands,
                        &mut sync_state.tombstones.snippet_commands,
                        &remote.tombstones.snippet_commands,
                        cfg.conflict_strategy,
                        |winner, _| winner,
                    );
                }
            }

            if cfg.sync_settings {
                if let Some(rset) = remote.settings {
                    settings_work = merge_settings_3way(
                        &settings_work,
                        &rset,
                        sync_state.last_synced_settings_updated_at.as_deref(),
                        cfg.conflict_strategy,
                    );
                }
            }

            if cfg.sync_keys && encrypted {
                final_remote_keys = remote.private_keys;
            }
        }

        if cfg.sync_servers || cfg.sync_groups {
            reconcile_server_groups(
                &servers_work.servers,
                &mut servers_work.groups,
                cfg.sync_servers,
                &now,
            );
        }

        prune_expired_tombstones(&mut sync_state.tombstones, now_dt);

        // 收集待上传的 SSH 私钥文件（仅在设置同步加密口令且开启 sync_keys 时附带）
        final_upload_keys.clear();
        if cfg.sync_keys && encrypted {
            if let Ok(keys_dir) = storage::get_keys_dir() {
                for server in &servers_work.servers {
                    let Some(raw_name) = server.private_key_filename.as_deref() else {
                        continue;
                    };
                    let Some(fname) = safe_key_filename(raw_name) else {
                        continue;
                    };
                    if final_upload_keys.contains_key(&fname) {
                        continue;
                    }
                    let disk_path = keys_dir.join(&fname);
                    if disk_path.is_file() {
                        if let Ok(bytes) = fs::read(&disk_path) {
                            final_upload_keys.insert(fname, BASE64.encode(bytes));
                        }
                    } else if let Some(b64) = final_remote_keys.get(&fname) {
                        final_upload_keys.insert(fname, b64.clone());
                    }
                }
            }
        }

        settings_work.sync.last_sync_at = Some(now.clone());

        let upload_bundle = SyncBundle {
            updated_at: now.clone(),
            servers: cfg.sync_servers.then(|| servers_work.servers.clone()),
            groups: cfg.sync_groups.then(|| servers_work.groups.clone()),
            snippets: cfg.sync_snippets.then(|| snippets_work.clone()),
            settings: cfg.sync_settings.then(|| {
                let mut s = settings_work.clone();
                s.sync.webdav_password.clear();
                s.sync.sync_passphrase.clear();
                s
            }),
            tombstones: sync_state.tombstones.clone(),
            private_keys: final_upload_keys.clone(),
        };

        if !had_remote && !dir_ensured {
            ensure_remote_dir_lazy(&client, cfg).await?;
            dir_ensured = true;
        }

        let payload = seal_bundle(&upload_bundle, &cfg.sync_passphrase)?;
        let (put_status, put_err_body) = put_bundle_with_etag(
            &client,
            &bundle_url,
            cfg,
            &payload,
            if attempt == 0 {
                remote_etag.as_deref()
            } else {
                None
            },
        )
        .await?;

        if put_status == 412 && attempt == 0 {
            tracing::info!("WebDAV ETag 并发冲突 (412)，重新拉取最新云端配置合并...");
            continue;
        }
        if !(200..300).contains(&put_status) {
            return Err(format_http_error(
                "上传同步文件失败",
                put_status,
                &put_err_body,
            ));
        }

        // 记录合并后基线
        if cfg.sync_servers {
            sync_state.known_servers = snapshot_known_map(&servers_work.servers);
        }
        if cfg.sync_groups {
            sync_state.known_groups = snapshot_known_map(&servers_work.groups);
        }
        if cfg.sync_snippets {
            sync_state.known_snippet_groups = snapshot_known_map(&snippets_work.groups);
            sync_state.known_snippet_commands = snapshot_known_map(&snippets_work.commands);
        }
        if cfg.sync_settings {
            sync_state.last_synced_settings_updated_at = settings_work.updated_at.clone();
        }

        final_servers = servers_work;
        final_snippets = snippets_work;
        final_settings = settings_work;
        final_sync_state = sync_state;
        break;
    }

    // 上传成功后再执行本地备份与落盘
    if cfg.backup_before_sync && had_remote {
        if let Err(e) = create_local_backup() {
            tracing::warn!("同步前创建本地备份失败: {}", e);
        }
    }

    // 还原远端同步过来的 SSH 私钥文件到本地 keys/
    let mut keys_synced = final_upload_keys.len();
    if cfg.sync_keys && encrypted {
        if let Ok(keys_dir) = storage::get_keys_dir() {
            for (raw_name, b64) in &final_remote_keys {
                let Some(fname) = safe_key_filename(raw_name) else {
                    continue;
                };
                // 仅恢复当前存活服务器引用或上传表中的私钥
                let referenced = final_servers
                    .servers
                    .iter()
                    .any(|s| s.private_key_filename.as_deref() == Some(fname.as_str()));
                if !referenced {
                    continue;
                }
                let target = keys_dir.join(&fname);
                if !target.exists() {
                    if let Ok(bytes) = BASE64.decode(b64.as_bytes()) {
                        if let Err(e) = storage::write_private_file(&target, &bytes) {
                            tracing::warn!("写入同步私钥 {} 失败: {}", fname, e);
                        } else {
                            keys_synced = keys_synced.max(final_upload_keys.len());
                        }
                    }
                }
            }
        }
    }

    if cfg.sync_servers || cfg.sync_groups {
        // 保障首次无改动时不丢本地磁盘上的 servers/groups
        if !cfg.sync_servers {
            final_servers.servers = local_servers_disk.servers;
        }
        if !cfg.sync_groups {
            final_servers.groups = local_servers_disk.groups;
        }
        storage::save_servers(&final_servers)
            .map_err(|e| format!("保存合并后的服务器列表失败: {e}"))?;
        local_servers_disk = final_servers.clone();
    }
    if cfg.sync_snippets {
        storage::save_snippets(&final_snippets)
            .map_err(|e| format!("保存合并后的代码片段失败: {e}"))?;
    }
    storage::save_settings(&final_settings).map_err(|e| format!("保存同步设置失败: {e}"))?;
    if let Err(e) = save_local_sync_state(&final_sync_state) {
        tracing::warn!("保存本地同步基线 sync_state.json 失败: {}", e);
    }

    LAST_SYNC_FINISHED_MS.store(now_epoch_ms(), Ordering::SeqCst);
    bump_data_revision();

    Ok(SyncReport {
        servers_count: local_servers_disk.servers.len(),
        groups_count: local_servers_disk.groups.len(),
        snippets_count: final_snippets.commands.len(),
        keys_synced,
        encrypted,
        synced_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_server(id: &str, label: &str, host: &str, updated_at: Option<&str>) -> ServerData {
        ServerData {
            id: id.to_string(),
            label: label.to_string(),
            host: host.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: updated_at.map(str::to_string),
            ..ServerData::default()
        }
    }

    fn mk_cmd(id: &str, name: &str, command: &str, updated_at: Option<&str>) -> SnippetCommand {
        SnippetCommand {
            id: id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: updated_at.map(str::to_string),
            ..SnippetCommand::default()
        }
    }

    #[test]
    fn joins_webdav_urls_cleanly() {
        assert_eq!(
            remote_bundle_url("https://dav.jianguoyun.com/dav/", "/shellmaster/").unwrap(),
            "https://dav.jianguoyun.com/dav/shellmaster/shellmaster-sync.json"
        );
        assert_eq!(
            remote_bundle_url("https://cloud.example.com/dav", "").unwrap(),
            "https://cloud.example.com/dav/shellmaster-sync.json"
        );
        assert!(join_webdav_url("ftp://example.com", "/a").is_err());
        assert!(join_webdav_url("", "/a").is_err());
    }

    #[test]
    fn encrypted_envelope_round_trips_and_rejects_wrong_passphrase() {
        let mut s = ServerData::default();
        s.id = "srv-1".into();
        s.label = "Prod".into();
        s.password_encrypted = Some("secret-pass".into());

        let mut keys = HashMap::new();
        keys.insert("123_id_ed25519".to_string(), BASE64.encode(b"ssh-key-bytes"));

        let bundle = SyncBundle {
            updated_at: "2026-11-28T00:00:00Z".into(),
            servers: Some(vec![s]),
            groups: None,
            snippets: None,
            settings: None,
            tombstones: TombstoneSet::default(),
            private_keys: keys,
        };

        let sealed = seal_bundle(&bundle, "my-sync-pass").unwrap();
        assert!(!sealed.contains("secret-pass"));
        assert!(!sealed.contains("c3NoLWtleS1ieXRlcw"));
        assert!(sealed.contains("\"mode\": \"encrypted\""));

        let (opened, was_enc) = open_bundle(&sealed, "my-sync-pass").unwrap();
        assert!(was_enc);
        assert_eq!(
            opened.servers.unwrap()[0].password_encrypted.as_deref(),
            Some("secret-pass")
        );
        assert_eq!(opened.private_keys.len(), 1);

        assert!(open_bundle(&sealed, "wrong-pass").is_err());
        assert!(open_bundle(&sealed, "").is_err());
    }

    #[test]
    fn plain_envelope_strips_secrets_and_private_keys() {
        let mut s = ServerData::default();
        s.id = "srv-1".into();
        s.password_encrypted = Some("should-not-leak".into());

        let mut keys = HashMap::new();
        keys.insert("123_id_ed25519".to_string(), BASE64.encode(b"private-key"));

        let bundle = SyncBundle {
            updated_at: "2026-11-28T00:00:00Z".into(),
            servers: Some(vec![s]),
            groups: None,
            snippets: None,
            settings: None,
            tombstones: TombstoneSet::default(),
            private_keys: keys,
        };

        let sealed = seal_bundle(&bundle, "").unwrap();
        assert!(!sealed.contains("should-not-leak"));
        assert!(!sealed.contains("123_id_ed25519"));
        let (opened, was_enc) = open_bundle(&sealed, "").unwrap();
        assert!(!was_enc);
        assert_eq!(opened.servers.unwrap()[0].password_encrypted, None);
        assert!(opened.private_keys.is_empty());
    }

    #[test]
    fn three_way_merge_propagates_local_and_remote_deletions_without_zombie_resurrection() {
        // 初始状态：A 和 B 曾同步过 s1、s2
        let s1_a = mk_server("s1", "Keep", "10.0.0.1", Some("2026-01-01T10:00:00Z"));
        let s1_b = s1_a.clone();
        let s2_b = mk_server("s2", "DeletedOnA", "10.0.0.2", Some("2026-01-01T10:00:00Z"));
        let s3_b = mk_server("s3", "NewOnB", "10.0.0.3", Some("2026-01-02T10:00:00Z"));

        let mut known = HashMap::new();
        known.insert("s1".into(), "2026-01-01T10:00:00Z".into());
        known.insert("s2".into(), "2026-01-01T10:00:00Z".into());

        // 设备 A 删除了 s2（带墓碑），远端 B 尚未删除 s2 但新增了 s3
        let mut local_tombstones = HashMap::new();
        local_tombstones.insert("s2".into(), "2026-01-02T09:00:00Z".into());
        let remote_tombstones = HashMap::new();

        let merged = merge_entities_3way(
            vec![s1_a],
            vec![s1_b, s2_b, s3_b],
            &known,
            &mut local_tombstones,
            &remote_tombstones,
            ConflictStrategy::Merge,
            merge_server_pair,
        );

        let ids: Vec<&str> = merged.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s3"]);
        assert!(local_tombstones.contains_key("s2"));

        // 若远端随后在删除时间之后重新创建/编辑了 s2，则允许更新胜出并解除墓碑
        let s2_recreated = mk_server("s2", "Reborn", "10.0.0.22", Some("2026-01-03T12:00:00Z"));
        let merged_reborn = merge_entities_3way(
            merged,
            vec![s2_recreated],
            &known,
            &mut local_tombstones,
            &HashMap::new(),
            ConflictStrategy::Merge,
            merge_server_pair,
        );
        assert!(merged_reborn.iter().any(|s| s.id == "s2" && s.host == "10.0.0.22"));
        assert!(!local_tombstones.contains_key("s2"));
    }

    #[test]
    fn three_way_merge_propagates_edits_and_honors_conflict_strategy() {
        let mut known = HashMap::new();
        known.insert("c1".into(), "2026-01-01T10:00:00Z".into());

        // 场景 1：仅远端 B 修改了命令内容，本机未改 -> 即便选 LocalFirst 也应采纳远端 B 的修改
        let c1_local_unchanged = mk_cmd("c1", "Disk", "df -h", Some("2026-01-01T10:00:00Z"));
        let c1_remote_edited = mk_cmd("c1", "Disk", "df -hT", Some("2026-01-02T10:00:00Z"));
        let non_conflict = merge_entities_3way(
            vec![c1_local_unchanged],
            vec![c1_remote_edited],
            &known,
            &mut HashMap::new(),
            &HashMap::new(),
            ConflictStrategy::LocalFirst,
            |w, _| w,
        );
        assert_eq!(non_conflict[0].command, "df -hT");

        // 场景 2：双端并发修改同一命令 -> 分别遵守 Merge（时间较新胜）、LocalFirst、RemoteFirst
        let c1_local_edit = mk_cmd("c1", "Disk", "df -h -local", Some("2026-01-02T08:00:00Z"));
        let c1_remote_edit = mk_cmd("c1", "Disk", "df -h -remote", Some("2026-01-02T09:00:00Z"));

        let res_merge = merge_entities_3way(
            vec![c1_local_edit.clone()],
            vec![c1_remote_edit.clone()],
            &known,
            &mut HashMap::new(),
            &HashMap::new(),
            ConflictStrategy::Merge,
            |w, _| w,
        );
        assert_eq!(res_merge[0].command, "df -h -remote");

        let res_local = merge_entities_3way(
            vec![c1_local_edit.clone()],
            vec![c1_remote_edit.clone()],
            &known,
            &mut HashMap::new(),
            &HashMap::new(),
            ConflictStrategy::LocalFirst,
            |w, _| w,
        );
        assert_eq!(res_local[0].command, "df -h -local");

        let res_remote = merge_entities_3way(
            vec![c1_local_edit],
            vec![c1_remote_edit],
            &known,
            &mut HashMap::new(),
            &HashMap::new(),
            ConflictStrategy::RemoteFirst,
            |w, _| w,
        );
        assert_eq!(res_remote[0].command, "df -h -remote");
    }

    #[test]
    fn merge_settings_preserves_machine_specific_paths_and_local_webdav_config() {
        let mut local = AppSettings::default();
        local.updated_at = Some("2026-01-01T10:00:00Z".into());
        local.terminal.font_size = 14;
        local.sftp.local_default_path = "/Users/alice/Downloads".into();
        local.sftp.external_editor_path = "/usr/local/bin/code".into();
        local.sync.webdav_password = "local-dav-password".into();
        local.sync.sync_passphrase = "local-passphrase".into();

        let mut remote = AppSettings::default();
        remote.updated_at = Some("2026-01-02T10:00:00Z".into());
        remote.terminal.font_size = 16;
        remote.sftp.local_default_path = r"C:\Users\bob\Downloads".into();
        remote.sftp.external_editor_path = r"C:\Program Files\VSCode\Code.exe".into();
        remote.sync.webdav_password.clear();

        let merged = merge_settings_3way(
            &local,
            &remote,
            Some("2026-01-01T10:00:00Z"),
            ConflictStrategy::Merge,
        );
        assert_eq!(merged.terminal.font_size, 16);
        assert_eq!(merged.sftp.local_default_path, "/Users/alice/Downloads");
        assert_eq!(merged.sftp.external_editor_path, "/usr/local/bin/code");
        assert_eq!(merged.sync.webdav_password, "local-dav-password");
        assert_eq!(merged.sync.sync_passphrase, "local-passphrase");
    }

    #[test]
    fn safe_key_filename_blocks_path_traversal() {
        assert_eq!(
            safe_key_filename("171000_id_ed25519").as_deref(),
            Some("171000_id_ed25519")
        );
        assert!(safe_key_filename("../evil").is_none());
        assert!(safe_key_filename("sub/key").is_none());
        assert!(safe_key_filename(r"sub\key").is_none());
        assert!(safe_key_filename("/etc/passwd").is_none());
        assert!(safe_key_filename(".hidden").is_none());
        assert!(safe_key_filename("C:key").is_none());
        assert!(safe_key_filename("").is_none());
    }

    #[test]
    fn summarizes_html_and_plain_http_error_bodies() {
        let html = "<html><head><title>500 Internal Server Error</title></head><body><h1>OneDrive upstream timeout</h1></body></html>";
        assert_eq!(
            format_http_error("读取远端同步文件失败", 500, html),
            "读取远端同步文件失败 (HTTP 500): 500 Internal Server Error OneDrive upstream timeout"
        );
        assert_eq!(
            format_http_error("读取远端同步文件失败", 500, "   \n  "),
            "读取远端同步文件失败 (HTTP 500)"
        );
    }

    #[test]
    fn lazy_mkdir_skips_virtual_mount_ancestors_and_only_creates_missing_leaf() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let seen_srv = seen.clone();

            let server_task = tokio::spawn(async move {
                for _ in 0..3 {
                    let (mut sock, _) = listener.accept().await.unwrap();
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("").to_string();
                    seen_srv.lock().unwrap().push(first_line.clone());

                    let status = match first_line.as_str() {
                        "PROPFIND /dav/OneDrive/Personal/shellmaster HTTP/1.1" => "404 Not Found",
                        "PROPFIND /dav/OneDrive/Personal HTTP/1.1" => "207 Multi-Status",
                        "MKCOL /dav/OneDrive/Personal/shellmaster HTTP/1.1" => "201 Created",
                        _ => "500 Unexpected Request",
                    };
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    sock.write_all(resp.as_bytes()).await.unwrap();
                }
            });

            let mut cfg = SyncSettings::default();
            cfg.webdav_url = format!("http://{addr}/dav");
            cfg.webdav_path = "OneDrive/Personal/shellmaster".into();

            let client = build_client(5).unwrap();
            ensure_remote_dir_lazy(&client, &cfg).await.unwrap();
            server_task.await.unwrap();

            let calls = seen.lock().unwrap().clone();
            assert_eq!(
                calls,
                vec![
                    "PROPFIND /dav/OneDrive/Personal/shellmaster HTTP/1.1",
                    "PROPFIND /dav/OneDrive/Personal HTTP/1.1",
                    "MKCOL /dav/OneDrive/Personal/shellmaster HTTP/1.1",
                ]
            );
        });
    }

    #[test]
    fn get_and_put_recover_from_transient_500_and_incompatible_if_match() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let sample_bundle = SyncBundle {
                updated_at: "2026-11-28T00:00:00Z".into(),
                ..SyncBundle::default()
            };
            let sealed_payload = seal_bundle(&sample_bundle, "pass123").unwrap();

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let body_for_get = sealed_payload.clone();
            let server_task = tokio::spawn(async move {
                // 1) 第一次 GET 返回 500（模拟挂载网盘正在回源刷新）
                // 2) 第二次 GET 返回 200 + ETag
                // 3) 第一次 PUT 带 If-Match 返回 500（模拟挂载网盘不兼容 If-Match 覆盖）
                // 4) 降级无条件 PUT 返回 204 成功
                for step in 0..4 {
                    let (mut sock, _) = listener.accept().await.unwrap();
                    let mut buf = vec![0u8; 8192];
                    let n = sock.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();

                    let resp = match step {
                        0 => {
                            let err = "upstream OneDrive busy";
                            format!(
                                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                err.len(),
                                err
                            )
                        }
                        1 => format!(
                            "HTTP/1.1 200 OK\r\nETag: \"etag-1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body_for_get.len(),
                            body_for_get
                        ),
                        2 => {
                            assert!(req.contains("if-match: \"etag-1\""));
                            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                        }
                        3 => {
                            assert!(!req.contains("if-match:"));
                            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                        }
                        _ => unreachable!(),
                    };
                    sock.write_all(resp.as_bytes()).await.unwrap();
                }
            });

            let mut cfg = SyncSettings::default();
            cfg.webdav_url = format!("http://{addr}/dav");
            cfg.webdav_path = "shellmaster".into();
            cfg.sync_passphrase = "pass123".into();

            let url = remote_bundle_url(&cfg.webdav_url, &cfg.webdav_path).unwrap();
            let client = build_client(5).unwrap();

            let (fetched, etag) = fetch_remote_bundle_with_retry(&client, &url, &cfg)
                .await
                .unwrap();
            assert!(fetched.is_some());
            assert_eq!(etag.as_deref(), Some("\"etag-1\""));

            let (put_code, _) =
                put_bundle_with_etag(&client, &url, &cfg, &sealed_payload, etag.as_deref())
                    .await
                    .unwrap();
            assert_eq!(put_code, 204);

            server_task.await.unwrap();
        });
    }
}
