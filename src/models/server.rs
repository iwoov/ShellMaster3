// Server, ServerGroup 数据结构

use serde::{Deserialize, Serialize};

// ============== 视图展示用的简化结构（兼容现有代码）==============

/// 服务器数据结构（用于视图展示）
#[derive(Clone)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub description: String,
    pub account: String,
    pub last_connected: String,
}

/// 服务器组（用于视图展示）
#[derive(Clone)]
pub struct ServerGroup {
    pub name: String,
    pub icon_path: &'static str,
    pub servers: Vec<Server>,
}

/// 侧边栏历史记录项
#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub name: String,
    pub time: String,
}

// ============== 持久化存储用的完整结构 ==============

/// 认证方式
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum AuthType {
    #[default]
    Password,
    PublicKey,
}

/// 代理类型
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ProxyType {
    #[default]
    Http,
    Socks5,
}

/// 代理配置（持久化用）
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password_encrypted: Option<String>,
}

/// 服务器数据（持久化用）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerData {
    pub id: String,
    pub group_id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub password_encrypted: Option<String>,
    pub private_key_path: Option<String>,
    pub key_passphrase_encrypted: Option<String>,
    pub jump_host_id: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub enable_monitor: bool,
    pub created_at: String,
    pub last_connected_at: Option<String>,
}

impl Default for ServerData {
    fn default() -> Self {
        Self {
            id: String::new(),
            group_id: None,
            label: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth_type: AuthType::Password,
            password_encrypted: None,
            private_key_path: None,
            key_passphrase_encrypted: None,
            jump_host_id: None,
            proxy: None,
            enable_monitor: true,
            created_at: String::new(),
            last_connected_at: None,
        }
    }
}

/// 服务器组（持久化用）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerGroupData {
    pub id: String,
    pub name: String,
    pub icon_path: String,
}

impl Default for ServerGroupData {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "默认分组".to_string(),
            icon_path: "icons/server.svg".to_string(),
        }
    }
}

/// 配置数据（用于存储到文件）
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub groups: Vec<ServerGroupData>,
    pub servers: Vec<ServerData>,
}
