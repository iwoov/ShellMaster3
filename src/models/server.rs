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
    pub os_name: Option<String>,
    /// 最近一次连接的时间；从未连接过时为 `None`
    pub last_connected_at: Option<chrono::NaiveDateTime>,
}

impl Server {
    /// `user@host:port` 形式的连接地址
    pub fn endpoint(&self) -> String {
        format!("{}@{}:{}", self.account, self.host, self.port)
    }
}

/// 服务器组（用于视图展示）
#[derive(Clone)]
pub struct ServerGroup {
    /// 分组 id；未分组的服务器归在 [`ServerGroup::UNGROUPED_ID`] 下
    pub id: String,
    pub name: String,
    pub servers: Vec<Server>,
}

impl ServerGroup {
    /// 「未分组」这一组的 id。保存服务器时分组名为空就不会建分组，所以它不会和真实分组的 id 重复
    pub const UNGROUPED_ID: &'static str = "";
}

/// 最近连接的记录项
#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub server_id: String,
    pub name: String,
    pub distro_icon: Option<&'static str>,
    /// `user@host` 形式的地址（不含端口）
    pub address: String,
    pub connected_at: chrono::NaiveDateTime,
}

// ============== 持久化存储用的完整结构 ==============

/// 认证方式
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum AuthType {
    #[default]
    Password,
    PublicKey,
    Agent,
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

    // 新字段：存储keys目录下的文件名（而非完整路径）
    #[serde(default)]
    pub private_key_filename: Option<String>,

    // 旧字段：向后兼容，存储完整路径（将在数据迁移后废弃）
    #[serde(default)]
    pub private_key_path: Option<String>,

    pub key_passphrase_encrypted: Option<String>,
    pub description: Option<String>,
    pub jump_host_id: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub enable_monitor: bool,
    /// 首次连接后由监控服务采集的操作系统发行版名称。
    #[serde(default)]
    pub os_name: Option<String>,
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
            private_key_filename: None,
            private_key_path: None,
            key_passphrase_encrypted: None,
            description: None,
            jump_host_id: None,
            proxy: None,
            enable_monitor: true,
            os_name: None,
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
