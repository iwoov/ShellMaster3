// SSH 连接配置

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::models::server::{AuthType, ProxyType as ServerProxyType, ServerData};
use crate::models::settings::{ConnectionSettings, GlobalProxyType};

use super::error::SshError;

/// 跳板机链的最大深度（防止配置过深）
const MAX_JUMP_DEPTH: usize = 5;

/// 心跳连续无响应多少次后判定连接已断开
const KEEPALIVE_MAX_MISSES: u32 = 3;

/// 启用压缩时的算法偏好（zlib@openssh.com 为认证后才开启的延迟压缩，OpenSSH 默认支持）
const COMPRESSION_PREFERRED: &[russh::compression::Name] = &[
    russh::compression::ZLIB_LEGACY,
    russh::compression::ZLIB,
    russh::compression::NONE,
];

/// 关闭压缩时只协商 none
const COMPRESSION_DISABLED: &[russh::compression::Name] = &[russh::compression::NONE];

/// SSH 连接配置
#[derive(Clone, Debug)]
pub struct SshConfig {
    /// 服务器名称（用于日志与错误信息）
    pub label: String,
    /// 目标主机
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式
    pub auth: AuthMethod,
    /// 连接超时（秒）。分别作用于 TCP/代理连接、SSH 握手和认证；
    /// 等待用户确认主机密钥的时间不计入。
    pub connect_timeout: u64,
    /// 跳板机：先连接并登录跳板机，再经它转发到本主机（可多级）
    pub jump_host: Option<Box<SshConfig>>,
    /// 代理：只用于直接建立 TCP 连接的那一跳，经跳板机转发时为 None
    pub proxy: Option<ProxyConfig>,
    /// 心跳配置
    pub keepalive: KeepaliveConfig,
    /// 是否启用 SSH 压缩
    pub compression: bool,
    /// 主机密钥校验策略（各跳共用）
    pub host_key_policy: HostKeyPolicy,
}

/// 认证方式
#[derive(Clone, Debug)]
pub enum AuthMethod {
    /// 密码认证。服务器只开放 keyboard-interactive 时，自动用该密码应答密码提示。
    Password(String),
    /// 公钥认证
    PublicKey {
        /// 私钥文件路径
        key_path: PathBuf,
        /// 私钥密码（如果有）
        passphrase: Option<String>,
    },
}

/// 代理类型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProxyType {
    #[default]
    Http,
    Socks4,
    Socks5,
}

impl ProxyType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Socks4 => "SOCKS4",
            Self::Socks5 => "SOCKS5",
        }
    }
}

/// 代理配置
#[derive(Clone, Debug)]
pub struct ProxyConfig {
    /// 代理类型
    pub proxy_type: ProxyType,
    /// 代理主机
    pub host: String,
    /// 代理端口
    pub port: u16,
    /// 认证信息（用户名, 密码）。SOCKS4 只使用用户名（user id）。
    pub auth: Option<(String, String)>,
}

/// 心跳配置
#[derive(Clone, Debug)]
pub struct KeepaliveConfig {
    /// 是否启用心跳
    pub enabled: bool,
    /// 心跳间隔（秒）
    pub interval: u64,
    /// 连续无响应多少次后判定断开
    pub max_retries: u32,
}

/// 主机密钥校验策略
#[derive(Clone, Debug, Default)]
pub struct HostKeyPolicy {
    /// 是否允许向用户弹出确认。自动重连时为 false：没法确认的密钥一律拒绝。
    pub interactive: bool,
    /// 严格模式：密钥与 known hosts 中保存的不一致时直接拒绝，不提供“仍然连接”。
    pub strict: bool,
    /// 本标签页之前已确认过的指纹（"host:port" -> SHA256 指纹）。
    /// 首次连接时选了“仅本次信任”的主机，重连时只接受同一把密钥。
    pub trusted: HashMap<String, String>,
}

/// known hosts / 已确认指纹表使用的主机标识
pub fn host_key_id(host: &str, port: u16) -> String {
    format!("{}:{}", host, port)
}

impl SshConfig {
    /// 根据保存的服务器配置构建连接配置。初次连接与自动重连共用这一个入口，保证两边行为一致。
    ///
    /// - 代理：优先使用服务器自己的代理；没有配置时使用全局代理。
    /// - 跳板机：`jump_host_id` 指向另一台已保存的服务器（兼容旧版本保存的 "host:port" 文本），
    ///   跳板机按它自己的认证和代理设置连接；经跳板机转发时本服务器自己的代理不生效。
    pub fn from_server(
        server: &ServerData,
        settings: &ConnectionSettings,
        servers: &[ServerData],
        policy: HostKeyPolicy,
    ) -> Result<Self, SshError> {
        let mut visited = HashSet::new();
        Self::build(server, settings, servers, &policy, &mut visited, 0)
    }

    fn build(
        server: &ServerData,
        settings: &ConnectionSettings,
        servers: &[ServerData],
        policy: &HostKeyPolicy,
        visited: &mut HashSet<String>,
        depth: usize,
    ) -> Result<Self, SshError> {
        if depth > MAX_JUMP_DEPTH {
            return Err(SshError::Config(format!(
                "jump host chain is deeper than {} levels",
                MAX_JUMP_DEPTH
            )));
        }
        if !visited.insert(server.id.clone()) {
            return Err(SshError::Config(format!(
                "jump host chain loops back to '{}'",
                server_display_name(server)
            )));
        }

        let jump_host = match jump_reference(server) {
            Some(reference) => {
                let jump_server = resolve_jump_server(reference, servers).ok_or_else(|| {
                    SshError::Config(format!(
                        "jump host '{}' of '{}' does not match any saved server; select it again in the server settings",
                        reference,
                        server_display_name(server)
                    ))
                })?;
                Some(Box::new(Self::build(
                    jump_server,
                    settings,
                    servers,
                    policy,
                    visited,
                    depth + 1,
                )?))
            }
            None => None,
        };

        // 经跳板机转发时这一跳不直接建立 TCP 连接，代理不适用
        let proxy = if jump_host.is_some() {
            None
        } else {
            server_proxy(server).or_else(|| global_proxy(settings))
        };

        Ok(Self {
            label: server.label.clone(),
            host: server.host.clone(),
            port: server.port,
            username: server.username.clone(),
            auth: auth_method(server),
            connect_timeout: settings.connection_timeout_secs.max(1) as u64,
            jump_host,
            proxy,
            keepalive: KeepaliveConfig {
                enabled: settings.keepalive_interval_secs > 0,
                interval: settings.keepalive_interval_secs as u64,
                max_retries: KEEPALIVE_MAX_MISSES,
            },
            compression: settings.compression,
            host_key_policy: policy.clone(),
        })
    }

    /// 用于日志和错误信息的名称（没有标签时用主机地址）
    pub fn display_name(&self) -> String {
        if self.label.trim().is_empty() {
            self.host.clone()
        } else {
            self.label.clone()
        }
    }

    /// 跳板机链，按连接顺序（最外层先连）排列
    pub fn jump_chain(&self) -> Vec<&SshConfig> {
        let mut chain = Vec::new();
        let mut current = self.jump_host.as_deref();
        while let Some(hop) = current {
            chain.push(hop);
            current = hop.jump_host.as_deref();
        }
        chain.reverse();
        chain
    }

    /// 实际建立 TCP 连接的那一跳所用的代理
    pub fn first_hop_proxy(&self) -> Option<&ProxyConfig> {
        let mut current = self;
        while let Some(next) = current.jump_host.as_deref() {
            current = next;
        }
        current.proxy.as_ref()
    }

    /// 构建 russh 配置
    pub fn to_russh_config(&self) -> russh::client::Config {
        let mut config = russh::client::Config::default();

        // 不使用 inactivity_timeout：它在一段时间没有任何流量时直接断开连接，
        // 而且发送心跳不会重置它。断线检测完全交给心跳：连续多次无响应才判定断开。
        config.inactivity_timeout = None;

        if self.keepalive.enabled && self.keepalive.interval > 0 {
            config.keepalive_interval = Some(Duration::from_secs(self.keepalive.interval));
            config.keepalive_max = self.keepalive.max_retries as usize;
        }

        config.preferred.compression = Cow::Borrowed(if self.compression {
            COMPRESSION_PREFERRED
        } else {
            COMPRESSION_DISABLED
        });

        // === 性能优化 ===
        // 增大窗口大小到 16MB，允许更多数据在等待确认前发送，提高高带宽连接的吞吐量
        config.window_size = 16 * 1024 * 1024;

        // NOTE:
        // SSH packet size must not exceed protocol/u16 bound (65535).
        // Setting 256KB triggers runtime errors in russh.
        config.maximum_packet_size = 32 * 1024;

        // 每个通道的接收队列长度。russh 在队列满时会阻塞整条连接的收包循环，
        // 留出更多余量，避免某个通道（如正在大量输出的终端）短暂处理不过来时拖住同连接的其他通道。
        config.channel_buffer_size = 128;

        config
    }
}

fn server_display_name(server: &ServerData) -> &str {
    if server.label.trim().is_empty() {
        &server.host
    } else {
        &server.label
    }
}

fn jump_reference(server: &ServerData) -> Option<&str> {
    server
        .jump_host_id
        .as_deref()
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
}

/// 解析跳板机引用：优先按服务器 ID 匹配；兼容旧版本保存的 "[user@]host[:port]" 文本，
/// 只有唯一匹配一台已保存的服务器时才采用，避免选错账号。
pub fn resolve_jump_server<'a>(
    reference: &str,
    servers: &'a [ServerData],
) -> Option<&'a ServerData> {
    let reference = reference.trim();
    if let Some(server) = servers.iter().find(|server| server.id == reference) {
        return Some(server);
    }

    let (user, host, port) = parse_legacy_jump_reference(reference);
    if host.is_empty() {
        return None;
    }
    let mut matches = servers.iter().filter(|server| {
        server.host.trim().eq_ignore_ascii_case(host)
            && port.map_or(true, |port| server.port == port)
            && user.map_or(true, |user| server.username == user)
    });
    match (matches.next(), matches.next()) {
        (Some(server), None) => Some(server),
        _ => None,
    }
}

/// 解析 "[user@]host[:port]"，支持 "[ipv6]:port"
fn parse_legacy_jump_reference(reference: &str) -> (Option<&str>, &str, Option<u16>) {
    let (user, rest) = match reference.rsplit_once('@') {
        Some((user, rest)) if !user.is_empty() => (Some(user), rest),
        _ => (None, reference),
    };

    if let Some(bracketed) = rest.strip_prefix('[') {
        if let Some((host, after)) = bracketed.split_once(']') {
            let port = after.strip_prefix(':').and_then(|port| port.parse().ok());
            return (user, host, port);
        }
    }

    match rest.rsplit_once(':') {
        // 只有一个冒号时才是 host:port；多个冒号视为不带端口的 IPv6 地址
        Some((host, port)) if !host.contains(':') => (user, host, port.parse().ok()),
        _ => (user, rest, None),
    }
}

/// 服务器自己配置的代理
fn server_proxy(server: &ServerData) -> Option<ProxyConfig> {
    let proxy = server.proxy.as_ref().filter(|proxy| proxy.enabled)?;
    Some(ProxyConfig {
        proxy_type: match proxy.proxy_type {
            ServerProxyType::Http => ProxyType::Http,
            ServerProxyType::Socks5 => ProxyType::Socks5,
        },
        host: proxy.host.trim().to_string(),
        port: proxy.port,
        auth: match (&proxy.username, &proxy.password_encrypted) {
            (Some(username), Some(password)) if !username.is_empty() => {
                Some((username.clone(), password.clone()))
            }
            _ => None,
        },
    })
}

/// 设置中的全局代理（服务器自己没有配置代理时使用）
fn global_proxy(settings: &ConnectionSettings) -> Option<ProxyConfig> {
    let proxy_type = match settings.global_proxy_type {
        GlobalProxyType::None => return None,
        GlobalProxyType::Http => ProxyType::Http,
        GlobalProxyType::Socks4 => ProxyType::Socks4,
        GlobalProxyType::Socks5 => ProxyType::Socks5,
    };
    let username = settings.global_proxy_username.trim();
    let password = &settings.global_proxy_password;
    // SOCKS4 只有用户 ID，没有密码
    let auth = if !username.is_empty() && (!password.is_empty() || proxy_type == ProxyType::Socks4)
    {
        Some((username.to_string(), password.clone()))
    } else {
        None
    };
    Some(ProxyConfig {
        proxy_type,
        host: settings.global_proxy_host.trim().to_string(),
        port: settings.global_proxy_port,
        auth,
    })
}

fn auth_method(server: &ServerData) -> AuthMethod {
    match server.auth_type {
        AuthType::Password => {
            AuthMethod::Password(server.password_encrypted.clone().unwrap_or_default())
        }
        AuthType::PublicKey => {
            // 优先使用 keys 目录下的文件名（新格式），回退到旧版本保存的完整路径
            let key_path = if let Some(filename) = &server.private_key_filename {
                crate::services::storage::get_keys_dir()
                    .map(|dir| dir.join(filename))
                    .unwrap_or_else(|_| filename.into())
            } else if let Some(old_path) = &server.private_key_path {
                old_path.into()
            } else {
                PathBuf::new()
            };

            AuthMethod::PublicKey {
                key_path,
                passphrase: server
                    .key_passphrase_encrypted
                    .clone()
                    .filter(|passphrase| !passphrase.is_empty()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::server::ProxyConfig as ServerProxyConfig;

    fn server(id: &str, host: &str, port: u16) -> ServerData {
        ServerData {
            id: id.to_string(),
            label: id.to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            password_encrypted: Some("secret".to_string()),
            ..ServerData::default()
        }
    }

    fn build(target: &ServerData, servers: &[ServerData]) -> Result<SshConfig, SshError> {
        SshConfig::from_server(
            target,
            &ConnectionSettings::default(),
            servers,
            HostKeyPolicy::default(),
        )
    }

    #[test]
    fn resolves_jump_host_by_id() {
        let bastion = server("bastion", "10.0.0.1", 22);
        let mut target = server("target", "192.168.1.10", 22);
        target.jump_host_id = Some("bastion".to_string());

        let config = build(&target, &[bastion.clone(), target.clone()]).unwrap();
        let jump = config.jump_host.as_deref().expect("jump host");
        assert_eq!(jump.host, "10.0.0.1");
        assert!(jump.jump_host.is_none());
    }

    #[test]
    fn resolves_legacy_host_port_reference() {
        let bastion = server("bastion", "jump.example.com", 2222);
        let mut target = server("target", "192.168.1.10", 22);
        target.jump_host_id = Some("root@jump.example.com:2222".to_string());

        let config = build(&target, &[bastion, target.clone()]).unwrap();
        assert_eq!(config.jump_host.unwrap().port, 2222);
    }

    #[test]
    fn unknown_or_ambiguous_jump_host_is_an_error_not_a_direct_connection() {
        let mut target = server("target", "192.168.1.10", 22);
        target.jump_host_id = Some("missing.example.com:22".to_string());
        assert!(build(&target, &[target.clone()]).is_err());

        // 两台服务器地址相同、无法确定用哪一台
        let a = server("a", "jump.example.com", 22);
        let b = server("b", "jump.example.com", 22);
        target.jump_host_id = Some("jump.example.com".to_string());
        assert!(build(&target, &[a, b, target.clone()]).is_err());
    }

    #[test]
    fn jump_host_loop_is_rejected() {
        let mut a = server("a", "a.example.com", 22);
        let mut b = server("b", "b.example.com", 22);
        a.jump_host_id = Some("b".to_string());
        b.jump_host_id = Some("a".to_string());
        assert!(build(&a, &[a.clone(), b]).is_err());
    }

    #[test]
    fn proxy_applies_only_to_first_hop() {
        let mut bastion = server("bastion", "10.0.0.1", 22);
        bastion.proxy = Some(ServerProxyConfig {
            enabled: true,
            proxy_type: ServerProxyType::Socks5,
            host: "127.0.0.1".to_string(),
            port: 1080,
            username: None,
            password_encrypted: None,
        });
        let mut target = server("target", "192.168.1.10", 22);
        target.jump_host_id = Some("bastion".to_string());
        target.proxy = bastion.proxy.clone();

        let config = build(&target, &[bastion, target.clone()]).unwrap();
        assert!(config.proxy.is_none(), "target is reached through the tunnel");
        let first_hop_proxy = config.first_hop_proxy().expect("first hop proxy");
        assert_eq!(first_hop_proxy.port, 1080);
        assert_eq!(first_hop_proxy.proxy_type, ProxyType::Socks5);
    }

    #[test]
    fn global_proxy_is_used_when_server_has_none() {
        let target = server("target", "192.168.1.10", 22);
        let settings = ConnectionSettings {
            global_proxy_type: GlobalProxyType::Socks4,
            global_proxy_host: "proxy.local".to_string(),
            global_proxy_port: 1080,
            global_proxy_username: "alice".to_string(),
            ..ConnectionSettings::default()
        };
        let config = SshConfig::from_server(
            &target,
            &settings,
            &[target.clone()],
            HostKeyPolicy::default(),
        )
        .unwrap();
        let proxy = config.proxy.expect("global proxy");
        assert_eq!(proxy.proxy_type, ProxyType::Socks4);
        assert_eq!(proxy.auth, Some(("alice".to_string(), String::new())));
    }

    #[test]
    fn russh_config_has_no_inactivity_timeout() {
        let target = server("target", "192.168.1.10", 22);
        let config = build(&target, &[target.clone()]).unwrap().to_russh_config();
        assert!(config.inactivity_timeout.is_none());
        assert!(config.keepalive_interval.is_some());
    }
}
