// SSH 连接配置

use std::path::PathBuf;

/// SSH 连接配置
#[derive(Clone, Debug)]
pub struct SshConfig {
    /// 目标主机
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式
    pub auth: AuthMethod,
    /// 连接超时（秒）
    pub connect_timeout: u64,
    /// 跳板机配置（预留）
    pub jump_host: Option<JumpHostConfig>,
    /// 代理配置（预留）
    pub proxy: Option<ProxyConfig>,
    /// 心跳配置
    pub keepalive: KeepaliveConfig,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: AuthMethod::Password(String::new()),
            connect_timeout: 30,
            jump_host: None,
            proxy: None,
            keepalive: KeepaliveConfig::default(),
        }
    }
}

/// 认证方式
#[derive(Clone, Debug)]
pub enum AuthMethod {
    /// 密码认证
    Password(String),
    /// 公钥认证
    PublicKey {
        /// 私钥文件路径
        key_path: PathBuf,
        /// 私钥密码（如果有）
        passphrase: Option<String>,
    },
    /// 交互式键盘认证（预留）
    KeyboardInteractive,
}

/// 跳板机配置（预留）
#[derive(Clone, Debug)]
pub struct JumpHostConfig {
    /// 跳板机主机
    pub host: String,
    /// 跳板机端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式
    pub auth: AuthMethod,
}

/// 代理类型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProxyType {
    #[default]
    Http,
    Socks5,
}

/// 代理配置（预留）
#[derive(Clone, Debug)]
pub struct ProxyConfig {
    /// 代理类型
    pub proxy_type: ProxyType,
    /// 代理主机
    pub host: String,
    /// 代理端口
    pub port: u16,
    /// 认证信息（用户名, 密码）
    pub auth: Option<(String, String)>,
}

/// 心跳配置
#[derive(Clone, Debug)]
pub struct KeepaliveConfig {
    /// 是否启用心跳
    pub enabled: bool,
    /// 心跳间隔（秒）
    pub interval: u64,
    /// 最大重试次数
    pub max_retries: u32,
}

impl Default for KeepaliveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: 60,
            max_retries: 3,
        }
    }
}

/// russh 客户端配置构建
impl SshConfig {
    /// 构建 russh 配置
    pub fn to_russh_config(&self) -> russh::client::Config {
        let mut config = russh::client::Config::default();
        // 设置不活动超时（russh 没有单独的 connection_timeout，我们用 inactivity_timeout）
        config.inactivity_timeout = Some(std::time::Duration::from_secs(self.connect_timeout));
        // 设置心跳
        if self.keepalive.enabled {
            config.keepalive_interval =
                Some(std::time::Duration::from_secs(self.keepalive.interval));
            config.keepalive_max = self.keepalive.max_retries as usize;
        }
        config
    }
}
