// Server, ServerGroup 数据结构

/// 服务器数据结构
#[derive(Clone)]
pub struct Server {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub description: String,
    pub account: String,
    pub last_connected: String,
}

/// 服务器组
#[derive(Clone)]
pub struct ServerGroup {
    pub name: String,
    pub icon_path: &'static str,
    pub servers: Vec<Server>,
}

/// 侧边栏历史记录项
#[derive(Clone)]
pub struct HistoryItem {
    pub name: String,
    pub time: String,
}
