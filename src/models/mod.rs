// 数据模型模块

pub mod connection;
pub mod server;
pub mod session;

pub use server::{
    AuthType, HistoryItem, ProxyConfig, ProxyType, Server, ServerConfig, ServerData, ServerGroup,
    ServerGroupData,
};
