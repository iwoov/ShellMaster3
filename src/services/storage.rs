// 本地数据持久化服务

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::models::{ServerConfig, ServerData, ServerGroupData};

/// 获取配置目录路径
pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("无法获取用户主目录")?;
    let config_dir = home.join(".shellmaster");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).context("无法创建配置目录")?;
    }
    Ok(config_dir)
}

/// 获取服务器配置文件路径
pub fn get_servers_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("servers.json"))
}

/// 加载服务器配置
pub fn load_servers() -> Result<ServerConfig> {
    let path = get_servers_file()?;
    if !path.exists() {
        // 返回默认配置
        return Ok(ServerConfig {
            groups: vec![ServerGroupData {
                id: uuid::Uuid::new_v4().to_string(),
                name: "默认分组".to_string(),
                icon_path: "icons/server.svg".to_string(),
            }],
            servers: vec![],
        });
    }
    let content = fs::read_to_string(&path).context("无法读取服务器配置文件")?;
    let config: ServerConfig = serde_json::from_str(&content).context("无法解析服务器配置文件")?;
    Ok(config)
}

/// 保存服务器配置
pub fn save_servers(config: &ServerConfig) -> Result<()> {
    let path = get_servers_file()?;
    let content = serde_json::to_string_pretty(config).context("无法序列化服务器配置")?;
    fs::write(&path, content).context("无法写入服务器配置文件")?;
    Ok(())
}

/// 添加服务器
pub fn add_server(server: ServerData) -> Result<()> {
    let mut config = load_servers()?;

    // 如果服务器指定了分组名但分组不存在，创建新分组
    if let Some(group_id) = &server.group_id {
        let group_exists = config.groups.iter().any(|g| g.id == *group_id);
        if !group_exists {
            // 使用 group_id 作为名称创建新分组
            config.groups.push(ServerGroupData {
                id: group_id.clone(),
                name: group_id.clone(),
                icon_path: "icons/server.svg".to_string(),
            });
        }
    }

    config.servers.push(server);
    save_servers(&config)?;
    Ok(())
}

/// 更新服务器
pub fn update_server(server: ServerData) -> Result<()> {
    let mut config = load_servers()?;
    if let Some(pos) = config.servers.iter().position(|s| s.id == server.id) {
        config.servers[pos] = server;
        save_servers(&config)?;
    }
    Ok(())
}

/// 删除服务器
pub fn delete_server(server_id: &str) -> Result<()> {
    let mut config = load_servers()?;
    config.servers.retain(|s| s.id != server_id);
    save_servers(&config)?;
    Ok(())
}

/// 获取所有分组
pub fn get_groups() -> Result<Vec<ServerGroupData>> {
    let config = load_servers()?;
    Ok(config.groups)
}

/// 添加分组
pub fn add_group(group: ServerGroupData) -> Result<()> {
    let mut config = load_servers()?;
    config.groups.push(group);
    save_servers(&config)?;
    Ok(())
}
