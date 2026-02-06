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

/// 获取密钥存储目录路径
/// macOS: ~/Library/Application Support/shellmaster/keys
/// Linux: ~/.config/shellmaster/keys
/// Windows: C:\Users\<用户名>\AppData\Roaming\shellmaster\keys
pub fn get_keys_dir() -> Result<PathBuf> {
    let keys_dir = get_config_dir()?.join("keys");

    if !keys_dir.exists() {
        fs::create_dir_all(&keys_dir).context("无法创建密钥目录")?;
    }

    // 设置目录权限为仅用户可读写 (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&keys_dir) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o700); // rwx------
            let _ = fs::set_permissions(&keys_dir, perms);
        }
    }

    Ok(keys_dir)
}

/// 存储私钥文件到应用密钥目录
/// 返回存储后的文件名（非完整路径）
pub fn store_private_key(source_path: &std::path::Path) -> Result<String> {
    let keys_dir = get_keys_dir()?;

    // 读取源文件
    let key_content =
        fs::read(source_path).with_context(|| format!("无法读取密钥文件: {:?}", source_path))?;

    // 生成唯一文件名：使用时间戳 + 原始文件名
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let original_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("id_rsa");

    let stored_filename = format!("{}_{}", timestamp, original_name);
    let stored_path = keys_dir.join(&stored_filename);

    // 写入密钥文件
    fs::write(&stored_path, key_content)
        .with_context(|| format!("无法写入密钥文件: {:?}", stored_path))?;

    // 设置文件权限为仅用户可读写 (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&stored_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600); // rw-------
            let _ = fs::set_permissions(&stored_path, perms);
        }
    }

    Ok(stored_filename)
}

/// 迁移旧的私钥路径到新的密钥目录
/// 应用启动时调用，将所有使用完整路径的私钥迁移到keys目录
pub fn migrate_legacy_private_keys() -> Result<()> {
    let mut config = load_servers()?;
    let mut updated = false;

    for server in &mut config.servers {
        // 如果已有新字段，跳过迁移
        if server.private_key_filename.is_some() {
            continue;
        }

        // 如果有旧的完整路径，尝试迁移
        if let Some(old_path_str) = server.private_key_path.clone() {
            let old_path = std::path::Path::new(&old_path_str);

            // 判断是否是完整路径（绝对路径）且文件存在
            if old_path.is_absolute() && old_path.exists() {
                match store_private_key(old_path) {
                    Ok(filename) => {
                        server.private_key_filename = Some(filename);
                        server.private_key_path = None; // 清除旧字段
                        updated = true;
                        tracing::info!("已迁移服务器 {} 的私钥: {}", server.label, old_path_str);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "无法迁移服务器 {} 的私钥: {} - 错误: {}",
                            server.label,
                            old_path_str,
                            e
                        );
                    }
                }
            }
        }
    }

    if updated {
        save_servers(&config)?;
        tracing::info!("私钥迁移完成");
    }

    Ok(())
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

/// 更新服务器的最后连接时间
pub fn update_server_last_connected(server_id: &str) -> Result<()> {
    let mut config = load_servers()?;
    if let Some(server) = config.servers.iter_mut().find(|s| s.id == server_id) {
        server.last_connected_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
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

// ======================== Snippets (快捷命令) 持久化 ========================

use crate::models::{SnippetCommand, SnippetGroup, SnippetsConfig};

/// 获取 Snippets 配置文件路径
pub fn get_snippets_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("snippets.json"))
}

/// 加载 Snippets 配置
pub fn load_snippets() -> Result<SnippetsConfig> {
    let path = get_snippets_file()?;
    if !path.exists() {
        return Ok(SnippetsConfig::default());
    }
    let content = fs::read_to_string(&path).context("无法读取 Snippets 配置文件")?;
    let config: SnippetsConfig =
        serde_json::from_str(&content).context("无法解析 Snippets 配置文件")?;
    Ok(config)
}

/// 保存 Snippets 配置
pub fn save_snippets(config: &SnippetsConfig) -> Result<()> {
    let path = get_snippets_file()?;
    let content = serde_json::to_string_pretty(config).context("无法序列化 Snippets 配置")?;
    fs::write(&path, content).context("无法写入 Snippets 配置文件")?;
    Ok(())
}

/// 添加命令组
pub fn add_snippet_group(group: SnippetGroup) -> Result<()> {
    let mut config = load_snippets()?;
    config.groups.push(group);
    save_snippets(&config)?;
    Ok(())
}

/// 更新命令组
pub fn update_snippet_group(group: SnippetGroup) -> Result<()> {
    let mut config = load_snippets()?;
    if let Some(pos) = config.groups.iter().position(|g| g.id == group.id) {
        config.groups[pos] = group;
        save_snippets(&config)?;
    }
    Ok(())
}

/// 删除命令组（级联删除子组和命令）
pub fn delete_snippet_group(group_id: &str) -> Result<()> {
    let mut config = load_snippets()?;

    // 收集所有要删除的组 ID（包括子组）
    let mut to_delete = vec![group_id.to_string()];
    let mut i = 0;
    while i < to_delete.len() {
        let parent_id = &to_delete[i];
        let children: Vec<String> = config
            .groups
            .iter()
            .filter(|g| g.parent_id.as_deref() == Some(parent_id))
            .map(|g| g.id.clone())
            .collect();
        to_delete.extend(children);
        i += 1;
    }

    // 删除组和相关命令
    config.groups.retain(|g| !to_delete.contains(&g.id));
    config.commands.retain(|c| {
        c.group_id
            .as_ref()
            .map_or(true, |gid| !to_delete.contains(gid))
    });

    save_snippets(&config)?;
    Ok(())
}

/// 添加命令
pub fn add_snippet_command(command: SnippetCommand) -> Result<()> {
    let mut config = load_snippets()?;
    config.commands.push(command);
    save_snippets(&config)?;
    Ok(())
}

/// 更新命令
pub fn update_snippet_command(command: SnippetCommand) -> Result<()> {
    let mut config = load_snippets()?;
    if let Some(pos) = config.commands.iter().position(|c| c.id == command.id) {
        config.commands[pos] = command;
        save_snippets(&config)?;
    }
    Ok(())
}

/// 删除命令
pub fn delete_snippet_command(command_id: &str) -> Result<()> {
    let mut config = load_snippets()?;
    config.commands.retain(|c| c.id != command_id);
    save_snippets(&config)?;
    Ok(())
}

// ======================== Known Hosts 持久化 ========================

use crate::models::{KnownHost, KnownHostsConfig};

/// 获取 Known Hosts 配置文件路径
pub fn get_known_hosts_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("known_hosts.json"))
}

/// 加载 Known Hosts 配置
pub fn load_known_hosts() -> Result<KnownHostsConfig> {
    let path = get_known_hosts_file()?;
    if !path.exists() {
        return Ok(KnownHostsConfig::default());
    }
    let content = fs::read_to_string(&path).context("无法读取 Known Hosts 配置文件")?;
    let config: KnownHostsConfig =
        serde_json::from_str(&content).context("无法解析 Known Hosts 配置文件")?;
    Ok(config)
}

/// 保存 Known Hosts 配置
pub fn save_known_hosts(config: &KnownHostsConfig) -> Result<()> {
    let path = get_known_hosts_file()?;
    let content = serde_json::to_string_pretty(config).context("无法序列化 Known Hosts 配置")?;
    fs::write(&path, content).context("无法写入 Known Hosts 配置文件")?;
    Ok(())
}

/// 查找已知主机（通过 host:port）
pub fn find_known_host(host: &str, port: u16) -> Result<Option<KnownHost>> {
    let config = load_known_hosts()?;
    let key = format!("{}:{}", host, port);
    Ok(config.hosts.into_iter().find(|h| h.host == key))
}

/// 添加已知主机
pub fn add_known_host(host: &str, port: u16, key_type: &str, fingerprint: &str) -> Result<()> {
    let mut config = load_known_hosts()?;
    let key = format!("{}:{}", host, port);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 如果已存在，更新 last_used
    if let Some(existing) = config.hosts.iter_mut().find(|h| h.host == key) {
        existing.last_used = now;
        existing.fingerprint = fingerprint.to_string();
        existing.key_type = key_type.to_string();
    } else {
        // 添加新条目
        config.hosts.push(KnownHost {
            host: key,
            key_type: key_type.to_string(),
            fingerprint: fingerprint.to_string(),
            first_seen: now.clone(),
            last_used: now,
        });
    }

    save_known_hosts(&config)?;
    Ok(())
}

/// 删除已知主机
pub fn remove_known_host(host: &str, port: u16) -> Result<()> {
    let mut config = load_known_hosts()?;
    let key = format!("{}:{}", host, port);
    config.hosts.retain(|h| h.host != key);
    save_known_hosts(&config)?;
    Ok(())
}

/// 更新已知主机的最后使用时间
pub fn update_known_host_last_used(host: &str, port: u16) -> Result<()> {
    let mut config = load_known_hosts()?;
    let key = format!("{}:{}", host, port);
    if let Some(h) = config.hosts.iter_mut().find(|h| h.host == key) {
        h.last_used = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        save_known_hosts(&config)?;
    }
    Ok(())
}
