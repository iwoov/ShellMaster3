// 本地数据持久化服务

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::models::{ServerConfig, ServerData, ServerGroupData};

/// 获取配置目录路径
/// macOS: ~/Library/Application Support/shellmaster
/// Linux: ~/.config/shellmaster
/// Windows: C:\Users\<用户名>\AppData\Roaming\shellmaster
pub fn get_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("无法获取系统配置目录")?
        .join("shellmaster");
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
        // 返回空配置，不创建默认分组
        return Ok(ServerConfig {
            groups: vec![],
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

    // 自动删除空分组（没有服务器的分组）
    let groups_with_servers: std::collections::HashSet<String> = config
        .servers
        .iter()
        .filter_map(|s| s.group_id.clone())
        .collect();
    config
        .groups
        .retain(|g| groups_with_servers.contains(&g.id));

    save_servers(&config)?;
    Ok(())
}

/// 获取所有分组
pub fn get_groups() -> Result<Vec<ServerGroupData>> {
    let config = load_servers()?;
    Ok(config.groups)
}

// ======================== Settings 配置持久化 ========================

use crate::models::AppSettings;

/// 获取设置配置文件路径
pub fn get_settings_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("settings.json"))
}

/// 加载应用设置
pub fn load_settings() -> Result<AppSettings> {
    let path = get_settings_file()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = fs::read_to_string(&path).context("无法读取设置配置文件")?;
    let settings: AppSettings = serde_json::from_str(&content).context("无法解析设置配置文件")?;
    Ok(settings)
}

/// 保存应用设置
pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = get_settings_file()?;
    let content = serde_json::to_string_pretty(settings).context("无法序列化设置配置")?;
    fs::write(&path, content).context("无法写入设置配置文件")?;
    Ok(())
}
