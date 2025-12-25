// SFTP 服务 - 封装 russh-sftp 客户端

use std::sync::Arc;

use russh_sftp::client::SftpSession;
use tracing::{debug, error, info};

use crate::models::sftp::{FileEntry, FileType};
use crate::ssh::session::SshSession;

/// SFTP 服务
/// 封装 russh-sftp 客户端，提供文件操作接口
#[derive(Clone)]
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

    /// 递归读取目录，返回所有文件条目（包含完整路径）
    ///
    /// # Arguments
    /// * `path` - 要遍历的目录路径
    ///
    /// # Returns
    /// * `Ok(Vec<FileEntry>)` - 所有文件和目录的列表（深度优先）
    /// * `Err(String)` - 读取失败
    pub async fn read_dir_recursive(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        info!("[SFTP] Reading directory recursively: {}", path);

        let mut all_entries = Vec::new();
        let mut dirs_to_process = vec![path.to_string()];

        while let Some(current_dir) = dirs_to_process.pop() {
            let entries = self.read_dir(&current_dir).await?;

            for entry in entries {
                if entry.is_dir() {
                    // 将子目录加入待处理队列
                    dirs_to_process.push(entry.path.clone());
                }
                all_entries.push(entry);
            }
        }

        info!(
            "[SFTP] Found {} entries recursively in {}",
            all_entries.len(),
            path
        );
        Ok(all_entries)
    }

    /// 递归创建目录（确保父目录存在）
    ///
    /// # Arguments
    /// * `path` - 要创建的目录路径
    ///
    /// # Returns
    /// * `Ok(())` - 创建成功
    /// * `Err(String)` - 创建失败
    pub async fn mkdir_recursive(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Creating directory recursively: {}", path);

        // 收集需要创建的所有路径段
        let mut paths_to_create = Vec::new();
        let mut current = path.to_string();

        // 从目标路径向上遍历，找出所有不存在的目录
        while !current.is_empty() && current != "/" {
            // 检查目录是否存在
            match self.sftp.metadata(&current).await {
                Ok(attrs) => {
                    if attrs.is_dir() {
                        // 目录已存在，停止向上遍历
                        break;
                    } else {
                        return Err(format!("Path {} exists but is not a directory", current));
                    }
                }
                Err(_) => {
                    // 目录不存在，需要创建
                    paths_to_create.push(current.clone());
                }
            }

            // 向上移动到父目录
            if let Some(parent) = current.rsplit_once('/').map(|(p, _)| p) {
                current = if parent.is_empty() {
                    "/".to_string()
                } else {
                    parent.to_string()
                };
            } else {
                break;
            }
        }

        // 从上往下创建目录
        paths_to_create.reverse();
        for dir_path in paths_to_create {
            debug!("[SFTP] Creating directory: {}", dir_path);
            self.mkdir(&dir_path).await?;
        }

        Ok(())
    }

    /// 下载文件到本地
    ///
    /// # Arguments
    /// * `remote_path` - 远程文件路径
    /// * `local_path` - 本地保存路径
    /// * `progress_callback` - 进度回调函数，参数为 (已传输字节数, 总字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(())` - 下载成功
    /// * `Err(String)` - 下载失败，包含错误信息
    pub async fn download_file<F>(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
        progress_callback: F,
    ) -> Result<(), String>
    where
        F: Fn(u64, u64, u64) + Send + 'static,
    {
        info!(
            "[SFTP] Downloading file: {} -> {:?}",
            remote_path, local_path
        );

        // 获取文件大小
        let attrs = self
            .sftp
            .metadata(remote_path)
            .await
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;

        let total_size = attrs.size.unwrap_or(0);
        if attrs.is_dir() {
            return Err("Cannot download a directory".to_string());
        }

        // 打开远程文件
        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|e| format!("Failed to open remote file: {}", e))?;

        // 创建本地文件
        let mut local_file = tokio::fs::File::create(local_path)
            .await
            .map_err(|e| format!("Failed to create local file: {}", e))?;

        // 读取并写入
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // 使用较大的 chunk size 以提高性能 (256KB)
        const CHUNK_SIZE: usize = 256 * 1024;
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut bytes_transferred: u64 = 0;

        // 速度计算变量
        let start_time = std::time::Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        loop {
            let bytes_read = remote_file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read from remote file: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            local_file
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(|e| format!("Failed to write to local file: {}", e))?;

            bytes_transferred += bytes_read as u64;

            // 计算速度（每100ms更新一次或更少）
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= 100 {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
            }

            // 调用进度回调（包含速度）
            progress_callback(bytes_transferred, total_size, current_speed);
        }

        // 确保所有数据都写入磁盘
        local_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush local file: {}", e))?;

        info!(
            "[SFTP] Download completed: {} ({} bytes)",
            remote_path, bytes_transferred
        );

        Ok(())
    }

    /// 下载文件的指定分片（用于多通道并行下载）
    ///
    /// # Arguments
    /// * `remote_path` - 远程文件路径
    /// * `local_file` - 本地文件句柄（线程安全，使用 seek 写入指定位置）
    /// * `offset` - 起始偏移量
    /// * `length` - 要下载的字节数
    /// * `progress_callback` - 进度回调函数，参数为 (本分片已传输字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(bytes_downloaded)` - 下载成功，返回实际下载的字节数
    /// * `Err(String)` - 下载失败，包含错误信息
    pub async fn download_chunk<F>(
        &self,
        remote_path: &str,
        local_file: std::sync::Arc<tokio::sync::Mutex<tokio::fs::File>>,
        offset: u64,
        length: u64,
        progress_callback: F,
    ) -> Result<u64, String>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        debug!(
            "[SFTP] Downloading chunk: {} offset={} length={}",
            remote_path, offset, length
        );

        // 打开远程文件
        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|e| format!("Failed to open remote file: {}", e))?;

        // Seek 到指定偏移量
        remote_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| format!("Failed to seek remote file: {}", e))?;

        // 读取并写入
        const CHUNK_SIZE: usize = 256 * 1024; // 256KB per read
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut bytes_transferred: u64 = 0;
        let mut remaining = length;

        // 速度计算变量
        let start_time = std::time::Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        while remaining > 0 {
            let to_read = std::cmp::min(CHUNK_SIZE as u64, remaining) as usize;
            let bytes_read = remote_file
                .read(&mut buffer[..to_read])
                .await
                .map_err(|e| format!("Failed to read from remote file: {}", e))?;

            if bytes_read == 0 {
                // 文件可能比预期短
                break;
            }

            // 写入本地文件（需要 seek 到正确位置）
            {
                let mut file = local_file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset + bytes_transferred))
                    .await
                    .map_err(|e| format!("Failed to seek local file: {}", e))?;
                file.write_all(&buffer[..bytes_read])
                    .await
                    .map_err(|e| format!("Failed to write to local file: {}", e))?;
            }

            bytes_transferred += bytes_read as u64;
            remaining -= bytes_read as u64;

            // 计算速度（每100ms更新一次）
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= 100 {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
            }

            // 调用进度回调
            progress_callback(bytes_transferred, current_speed);
        }

        debug!(
            "[SFTP] Chunk download completed: {} offset={} downloaded={}",
            remote_path, offset, bytes_transferred
        );

        Ok(bytes_transferred)
    }

    /// 上传本地文件到远程服务器
    ///
    /// # Arguments
    /// * `local_path` - 本地文件路径
    /// * `remote_path` - 远程保存路径
    /// * `progress_callback` - 进度回调函数，参数为 (已传输字节数, 总字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(())` - 上传成功
    /// * `Err(String)` - 上传失败，包含错误信息
    pub async fn upload_file<F>(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
        progress_callback: F,
    ) -> Result<(), String>
    where
        F: Fn(u64, u64, u64) + Send + 'static,
    {
        info!("[SFTP] Uploading file: {:?} -> {}", local_path, remote_path);

        // 获取本地文件大小
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|e| format!("Failed to get local file metadata: {}", e))?;

        let total_size = metadata.len();
        if metadata.is_dir() {
            return Err("Cannot upload a directory".to_string());
        }

        // 打开本地文件
        let mut local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| format!("Failed to open local file: {}", e))?;

        // 创建远程文件
        let mut remote_file = self
            .sftp
            .create(remote_path)
            .await
            .map_err(|e| format!("Failed to create remote file: {}", e))?;

        // 读取并写入
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // 使用较大的 chunk size 以提高性能 (256KB)
        const CHUNK_SIZE: usize = 256 * 1024;
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut bytes_transferred: u64 = 0;

        // 速度计算变量
        let start_time = std::time::Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        loop {
            let bytes_read = local_file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read from local file: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            remote_file
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(|e| format!("Failed to write to remote file: {}", e))?;

            bytes_transferred += bytes_read as u64;

            // 计算速度（每100ms更新一次或更少）
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= 100 {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
            }

            // 调用进度回调（包含速度）
            progress_callback(bytes_transferred, total_size, current_speed);
        }

        // 确保所有数据都写入远程
        remote_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush remote file: {}", e))?;

        info!(
            "[SFTP] Upload completed: {:?} ({} bytes)",
            local_path, bytes_transferred
        );

        Ok(())
    }

    /// 上传文件的指定分片（用于多通道并行上传）
    ///
    /// # Arguments
    /// * `local_file` - 本地文件句柄（线程安全，使用 seek 读取指定位置）
    /// * `remote_path` - 远程文件路径
    /// * `offset` - 起始偏移量
    /// * `length` - 要上传的字节数
    /// * `progress_callback` - 进度回调函数，参数为 (本分片已传输字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(bytes_uploaded)` - 上传成功，返回实际上传的字节数
    /// * `Err(String)` - 上传失败，包含错误信息
    pub async fn upload_chunk<F>(
        &self,
        local_file: std::sync::Arc<tokio::sync::Mutex<tokio::fs::File>>,
        remote_path: &str,
        offset: u64,
        length: u64,
        progress_callback: F,
    ) -> Result<u64, String>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        debug!(
            "[SFTP] Uploading chunk: {} offset={} length={}",
            remote_path, offset, length
        );

        // 打开远程文件用于写入（需要使用特殊标志来支持 seek 写入）
        let mut remote_file = self
            .sftp
            .open_with_flags(remote_path, russh_sftp::protocol::OpenFlags::WRITE)
            .await
            .map_err(|e| format!("Failed to open remote file for writing: {}", e))?;

        // Seek 到指定偏移量
        remote_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| format!("Failed to seek remote file: {}", e))?;

        // 读取并写入
        const CHUNK_SIZE: usize = 256 * 1024; // 256KB per write
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut bytes_transferred: u64 = 0;
        let mut remaining = length;

        // 速度计算变量
        let start_time = std::time::Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        while remaining > 0 {
            let to_read = std::cmp::min(CHUNK_SIZE as u64, remaining) as usize;

            // 从本地文件读取（需要 seek 到正确位置）
            let bytes_read = {
                let mut file = local_file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset + bytes_transferred))
                    .await
                    .map_err(|e| format!("Failed to seek local file: {}", e))?;
                file.read(&mut buffer[..to_read])
                    .await
                    .map_err(|e| format!("Failed to read from local file: {}", e))?
            };

            if bytes_read == 0 {
                // 文件可能比预期短
                break;
            }

            // 写入远程文件
            remote_file
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(|e| format!("Failed to write to remote file: {}", e))?;

            bytes_transferred += bytes_read as u64;
            remaining -= bytes_read as u64;

            // 计算速度（每100ms更新一次）
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= 100 {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
            }

            // 调用进度回调
            progress_callback(bytes_transferred, current_speed);
        }

        // 确保数据写入
        remote_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush remote file: {}", e))?;

        debug!(
            "[SFTP] Chunk upload completed: {} offset={} uploaded={}",
            remote_path, offset, bytes_transferred
        );

        Ok(bytes_transferred)
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
