// Known Hosts 数据模型
// 用于存储和验证 SSH 服务器公钥指纹

use serde::{Deserialize, Serialize};

/// 已知主机条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnownHost {
    /// 主机地址（host:port 格式）
    pub host: String,
    /// 密钥类型（ssh-ed25519, ssh-rsa 等）
    pub key_type: String,
    /// SHA256 指纹
    pub fingerprint: String,
    /// 首次连接时间
    pub first_seen: String,
    /// 最后使用时间
    pub last_used: String,
}

/// Known Hosts 配置
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KnownHostsConfig {
    pub hosts: Vec<KnownHost>,
}
