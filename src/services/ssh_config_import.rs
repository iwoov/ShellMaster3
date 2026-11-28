//! 从 OpenSSH `~/.ssh/config` 解析并导入服务器配置。
//!
//! 支持 `Host`、`HostName`、`User`、`Port`、`IdentityFile`、`ProxyJump` 以及 `Host *` 默认值；
//! 自动跳过通配符主机别名、与现有主机去重，并将存在的私钥复制到托管的 `keys/` 目录。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::models::server::AuthType;
use crate::models::{ServerConfig, ServerData};
use crate::services::storage;

/// 从 `~/.ssh/config` 解析出的单条主机记录
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSshHost {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub identity_file: Option<PathBuf>,
    pub proxy_jump: Option<String>,
}

/// 导入结果统计
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Default, Clone)]
struct RawBlock {
    patterns: Vec<String>,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
}

fn strip_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

fn split_directive(line: &str) -> Option<(String, String)> {
    // 去掉行内注释（仅处理不在引号中的 #）
    let mut in_quote = false;
    let mut clean_end = line.len();
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            '#' if !in_quote => {
                clean_end = idx;
                break;
            }
            _ => {}
        }
    }
    let trimmed = line[..clean_end].trim();
    if trimmed.is_empty() {
        return None;
    }

    // OpenSSH 支持 `Key Value` 和 `Key=Value`
    let sep_idx = trimmed
        .find(|c: char| c.is_whitespace() || c == '=')
        .filter(|&i| i > 0)?;
    let key = trimmed[..sep_idx].to_ascii_lowercase();
    let rest = trimmed[sep_idx..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    let value = rest.trim();
    if value.is_empty() {
        return None;
    }
    Some((key, value.to_string()))
}

fn expand_tilde(path_str: &str, home_dir: Option<&Path>) -> PathBuf {
    let cleaned = strip_quotes(path_str);
    if let Some(rest) = cleaned.strip_prefix("~/") {
        if let Some(home) = home_dir {
            return home.join(rest);
        }
    } else if cleaned == "~" {
        if let Some(home) = home_dir {
            return home.to_path_buf();
        }
    }
    PathBuf::from(cleaned)
}

fn is_concrete_alias(pattern: &str) -> bool {
    !pattern.is_empty()
        && !pattern.starts_with('!')
        && !pattern.contains('*')
        && !pattern.contains('?')
}

/// 解析 OpenSSH config 文本内容为结构化主机列表
pub fn parse_ssh_config(content: &str, home_dir: Option<&Path>, default_user: &str) -> Vec<ParsedSshHost> {
    let mut blocks: Vec<RawBlock> = Vec::new();
    let mut current: Option<RawBlock> = None;

    for line in content.lines() {
        let Some((key, value)) = split_directive(line) else {
            continue;
        };
        if key == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let patterns = value
                .split_whitespace()
                .map(|p| strip_quotes(p).to_string())
                .filter(|p| !p.is_empty())
                .collect();
            current = Some(RawBlock {
                patterns,
                ..RawBlock::default()
            });
            continue;
        }
        if key == "match" {
            // 遇到 Match 块时结束当前 Host 块，避免把条件配置混入前一个主机
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };
        match key.as_str() {
            "hostname" => {
                if block.hostname.is_none() {
                    block.hostname = Some(strip_quotes(&value).to_string());
                }
            }
            "user" => {
                if block.user.is_none() {
                    block.user = Some(strip_quotes(&value).to_string());
                }
            }
            "port" => {
                if block.port.is_none() {
                    block.port = strip_quotes(&value).parse::<u16>().ok().filter(|&p| p > 0);
                }
            }
            "identityfile" => {
                if block.identity_file.is_none() {
                    block.identity_file = Some(strip_quotes(&value).to_string());
                }
            }
            "proxyjump" => {
                if block.proxy_jump.is_none() {
                    let jump = strip_quotes(&value);
                    if !jump.eq_ignore_ascii_case("none") {
                        // 多跳链取第一跳别名，若写成 user@host:port 则提取 host 部分
                        let first = jump.split(',').next().unwrap_or(jump).trim();
                        let without_user = first.rsplit_once('@').map(|(_, h)| h).unwrap_or(first);
                        let host_alias = without_user
                            .split_once(':')
                            .map(|(h, _)| h)
                            .unwrap_or(without_user)
                            .trim();
                        if !host_alias.is_empty() {
                            block.proxy_jump = Some(host_alias.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }

    // 汇总 `Host *` 全局默认值（OpenSSH 中先匹配者生效）
    let mut global_user: Option<String> = None;
    let mut global_port: Option<u16> = None;
    let mut global_identity: Option<String> = None;
    for block in &blocks {
        if block.patterns.iter().any(|p| p == "*") {
            if global_user.is_none() {
                global_user = block.user.clone();
            }
            if global_port.is_none() {
                global_port = block.port;
            }
            if global_identity.is_none() {
                global_identity = block.identity_file.clone();
            }
        }
    }

    let mut result = Vec::new();
    let mut seen_aliases = std::collections::HashSet::new();
    for block in blocks {
        for alias in &block.patterns {
            if !is_concrete_alias(alias) || !seen_aliases.insert(alias.clone()) {
                continue;
            }
            let host = block
                .hostname
                .clone()
                .unwrap_or_else(|| alias.clone());
            let port = block.port.or(global_port).unwrap_or(22);
            let username = block
                .user
                .clone()
                .or_else(|| global_user.clone())
                .unwrap_or_else(|| default_user.to_string());
            let identity_file = block
                .identity_file
                .as_deref()
                .or(global_identity.as_deref())
                .map(|p| expand_tilde(p, home_dir));

            result.push(ParsedSshHost {
                alias: alias.clone(),
                host,
                port,
                username,
                identity_file,
                proxy_jump: block.proxy_jump.clone(),
            });
        }
    }
    result
}

/// 将解析出的主机合并进现有 `ServerConfig`（去重 + 自动关联 ProxyJump 跳板机 ID）
pub fn merge_parsed_hosts<F>(
    config: &mut ServerConfig,
    parsed: &[ParsedSshHost],
    group_name: Option<&str>,
    mut import_key: F,
) -> ImportSummary
where
    F: FnMut(&Path) -> Option<String>,
{
    let mut summary = ImportSummary::default();
    let mut alias_to_id: HashMap<String, String> = HashMap::new();
    for server in &config.servers {
        alias_to_id.insert(server.label.clone(), server.id.clone());
    }

    let mut pending_jumps: Vec<(String, String)> = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for entry in parsed {
        let already_exists = config.servers.iter().any(|s| {
            s.label.eq_ignore_ascii_case(&entry.alias)
                || (s.host.eq_ignore_ascii_case(&entry.host)
                    && s.port == entry.port
                    && s.username == entry.username)
        });
        if already_exists {
            summary.skipped += 1;
            continue;
        }

        let key_filename = entry
            .identity_file
            .as_deref()
            .and_then(&mut import_key);
        let auth_type = if key_filename.is_some() {
            AuthType::PublicKey
        } else {
            AuthType::Agent
        };

        let id = uuid::Uuid::new_v4().to_string();
        alias_to_id.insert(entry.alias.clone(), id.clone());
        if let Some(jump_alias) = &entry.proxy_jump {
            pending_jumps.push((id.clone(), jump_alias.clone()));
        }

        let group_id = group_name.map(str::to_string);
        if let Some(gid) = &group_id {
            if !config.groups.iter().any(|g| &g.id == gid) {
                config.groups.push(crate::models::ServerGroupData {
                    id: gid.clone(),
                    name: gid.clone(),
                    icon_path: "icons/server.svg".to_string(),
                    updated_at: Some(now.clone()),
                });
            }
        }

        config.servers.push(ServerData {
            id,
            group_id,
            label: entry.alias.clone(),
            host: entry.host.clone(),
            port: entry.port,
            username: entry.username.clone(),
            auth_type,
            private_key_filename: key_filename,
            created_at: now.clone(),
            updated_at: Some(now.clone()),
            ..ServerData::default()
        });
        summary.imported += 1;
    }

    // 第二遍回填 ProxyJump 关联的跳板机 server id
    for (server_id, jump_alias) in pending_jumps {
        if let Some(jump_id) = alias_to_id.get(&jump_alias).cloned() {
            if jump_id != server_id {
                if let Some(server) = config.servers.iter_mut().find(|s| s.id == server_id) {
                    server.jump_host_id = Some(jump_id);
                }
            }
        }
    }

    summary
}

/// 获取当前用户默认的 `~/.ssh/config` 路径
pub fn default_ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh").join("config"))
}

/// 从指定 OpenSSH 配置文件导入主机到本地 `servers.json`
pub fn import_from_ssh_config_path(path: &Path) -> Result<ImportSummary> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("无法读取 SSH 配置文件: {}", path.display()))?;
    let default_user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string());
    let home = dirs::home_dir();
    let parsed = parse_ssh_config(&content, home.as_deref(), &default_user);

    let mut config = storage::load_servers()?;
    let summary = merge_parsed_hosts(&mut config, &parsed, Some("SSH Config"), |key_path| {
        if key_path.is_file() {
            storage::store_private_key(key_path).ok()
        } else {
            None
        }
    });
    if summary.imported > 0 {
        storage::save_servers(&config)?;
        crate::services::sync::notify_local_data_changed();
    }
    Ok(summary)
}

/// 从默认 `~/.ssh/config` 导入主机
pub fn import_default_ssh_config() -> Result<Option<ImportSummary>> {
    let Some(path) = default_ssh_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    import_from_ssh_config_path(&path).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_defaults_wildcards_and_proxy_jump() {
        let sample = r#"
# Global defaults
Host *
    User ops
    Port 2222
    IdentityFile ~/.ssh/id_ed25519

Host bastion
    HostName 198.51.100.24
    User jump
    Port=22

Host prod-api-01 prod-api-02 *.internal
    HostName 10.0.12.8 # inline comment
    ProxyJump jump@bastion:22

Match host foo
    User ignored
"#;
        let home = Path::new("/home/alice");
        let hosts = parse_ssh_config(sample, Some(home), "alice");
        assert_eq!(hosts.len(), 3);

        assert_eq!(hosts[0].alias, "bastion");
        assert_eq!(hosts[0].host, "198.51.100.24");
        assert_eq!(hosts[0].user(), "jump");
        assert_eq!(hosts[0].port, 22);
        assert_eq!(
            hosts[0].identity_file.as_deref(),
            Some(Path::new("/home/alice/.ssh/id_ed25519"))
        );

        assert_eq!(hosts[1].alias, "prod-api-01");
        assert_eq!(hosts[1].host, "10.0.12.8");
        assert_eq!(hosts[1].username, "ops");
        assert_eq!(hosts[1].port, 2222);
        assert_eq!(hosts[1].proxy_jump.as_deref(), Some("bastion"));

        assert_eq!(hosts[2].alias, "prod-api-02");
    }

    #[test]
    fn merges_with_dedup_and_resolves_proxy_jump_ids() {
        let parsed = vec![
            ParsedSshHost {
                alias: "bastion".into(),
                host: "198.51.100.24".into(),
                port: 22,
                username: "jump".into(),
                identity_file: Some(PathBuf::from("/tmp/id_ed25519")),
                proxy_jump: None,
            },
            ParsedSshHost {
                alias: "app-01".into(),
                host: "10.0.12.8".into(),
                port: 22,
                username: "deploy".into(),
                identity_file: None,
                proxy_jump: Some("bastion".into()),
            },
        ];

        let mut cfg = ServerConfig::default();
        let first = merge_parsed_hosts(&mut cfg, &parsed, Some("SSH Config"), |_| {
            Some("imported_key".into())
        });
        assert_eq!(first, ImportSummary { imported: 2, skipped: 0 });
        assert_eq!(cfg.servers[0].auth_type, AuthType::PublicKey);
        assert_eq!(cfg.servers[1].auth_type, AuthType::Agent);
        assert_eq!(
            cfg.servers[1].jump_host_id.as_deref(),
            Some(cfg.servers[0].id.as_str())
        );

        // 再次导入同样列表应全部跳过
        let second = merge_parsed_hosts(&mut cfg, &parsed, Some("SSH Config"), |_| None);
        assert_eq!(second, ImportSummary { imported: 0, skipped: 2 });
        assert_eq!(cfg.servers.len(), 2);
    }

    trait HostExt {
        fn user(&self) -> &str;
    }
    impl HostExt for ParsedSshHost {
        fn user(&self) -> &str {
            &self.username
        }
    }
}
