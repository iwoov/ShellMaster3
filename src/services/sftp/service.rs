// SFTP 服务 - 封装 russh-sftp 客户端

use std::sync::Arc;

use russh_sftp::client::SftpSession;
use tracing::{debug, error, info};

use crate::models::sftp::{FileEntry, FileType};
use crate::ssh::session::SshSession;

/// SFTP 服务
/// 封装 russh-sftp 客户端，提供文件操作接口
pub struct SftpService {
    /// 会话 ID
    session_id: String,
    /// russh-sftp 客户端会话（线程安全）
    sftp: Arc<SftpSession>,
}

impl SftpService {
    /// 创建 SFTP 服务
    pub async fn new(session_id: String, ssh_session: &Arc<SshSession>) -> Result<Self, String> {
        info!("[SFTP] Creating SFTP service for session {}", session_id);

        // 打开 SFTP 子系统通道
        let channel = ssh_session
            .handle()
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open channel: {}", e))?;

        // 请求 SFTP 子系统
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("Failed to request sftp subsystem: {}", e))?;

        // 使用 russh-sftp 包装通道
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("Failed to create SFTP session: {}", e))?;

        info!("[SFTP] SFTP service created for session {}", session_id);

        Ok(Self {
            session_id,
            sftp: Arc::new(sftp),
        })
    }

    /// 获取会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 获取 SFTP 会话引用（用于并发操作）
    pub fn sftp(&self) -> Arc<SftpSession> {
        self.sftp.clone()
    }

    /// 获取用户主目录
    pub async fn get_home_dir(&self) -> Result<String, String> {
        // 尝试通过 realpath 获取 ~ 的真实路径
        match self.sftp.canonicalize(".").await {
            Ok(path) => {
                // russh-sftp canonicalize 返回 String
                debug!("[SFTP] Home directory: {}", path);
                Ok(path)
            }
            Err(e) => {
                error!("[SFTP] Failed to get home directory: {}", e);
                // 回退到根目录
                Ok("/".to_string())
            }
        }
    }

    /// 读取目录内容
    pub async fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        debug!("[SFTP] Reading directory: {}", path);

        let dir = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|e| format!("Failed to read directory {}: {}", path, e))?;

        let mut entries = Vec::new();
        for entry in dir {
            let name = entry.file_name();

            // 跳过 . 和 ..
            if name == "." || name == ".." {
                continue;
            }

            let full_path = if path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", path.trim_end_matches('/'), name)
            };

            let attrs = entry.metadata();

            // 确定文件类型
            let file_type = if attrs.is_dir() {
                FileType::Directory
            } else if attrs.is_symlink() {
                FileType::Symlink
            } else {
                // russh-sftp 没有 is_file()，默认为普通文件
                FileType::File
            };

            let mut file_entry = FileEntry::new(name.to_string(), full_path, file_type);
            file_entry.size = attrs.size.unwrap_or(0);
            file_entry.permissions = attrs.permissions.map(|p| p as u32).unwrap_or(0);
            file_entry.uid = attrs.uid;
            file_entry.gid = attrs.gid;

            // 修改时间
            if let Some(mtime) = attrs.mtime {
                file_entry.modified =
                    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64));
            }

            entries.push(file_entry);
        }

        debug!("[SFTP] Read {} entries from {}", entries.len(), path);
        Ok(entries)
    }

    /// 读取文件内容（用于读取 /etc/passwd 和 /etc/group）
    pub async fn read_file(&self, path: &str) -> Result<String, String> {
        debug!("[SFTP] Reading file: {}", path);

        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file {}: {}", path, e))?;

        use tokio::io::AsyncReadExt;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(|e| format!("Failed to read file {}: {}", path, e))?;

        debug!("[SFTP] Read {} bytes from {}", content.len(), path);
        Ok(content)
    }

    /// 创建目录
    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Creating directory: {}", path);
        self.sftp
            .create_dir(path)
            .await
            .map_err(|e| format!("Failed to create directory {}: {}", path, e))
    }

    /// 删除文件
    pub async fn remove_file(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Removing file: {}", path);
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| format!("Failed to remove file {}: {}", path, e))
    }

    /// 删除目录
    pub async fn remove_dir(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Removing directory: {}", path);
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| format!("Failed to remove directory {}: {}", path, e))
    }

    /// 重命名文件或目录
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        info!("[SFTP] Renaming {} -> {}", from, to);
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| format!("Failed to rename {} to {}: {}", from, to, e))
    }

    /// 获取文件/目录属性
    pub async fn stat(&self, path: &str) -> Result<FileEntry, String> {
        debug!("[SFTP] Getting stat for: {}", path);

        let attrs = self
            .sftp
            .metadata(path)
            .await
            .map_err(|e| format!("Failed to stat {}: {}", path, e))?;

        let name = path.rsplit('/').next().unwrap_or(path).to_string();

        let file_type = if attrs.is_dir() {
            FileType::Directory
        } else if attrs.is_symlink() {
            FileType::Symlink
        } else {
            // russh-sftp 没有 is_file()，默认为普通文件
            FileType::File
        };

        let mut entry = FileEntry::new(name, path.to_string(), file_type);
        entry.size = attrs.size.unwrap_or(0);
        entry.permissions = attrs.permissions.map(|p| p as u32).unwrap_or(0);
        entry.uid = attrs.uid;
        entry.gid = attrs.gid;

        if let Some(mtime) = attrs.mtime {
            entry.modified =
                Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64));
        }

        Ok(entry)
    }
}

impl Drop for SftpService {
    fn drop(&mut self) {
        info!(
            "[SFTP] Dropping SFTP service for session {}",
            self.session_id
        );
    }
}
