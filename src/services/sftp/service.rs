// SFTP 服务 - 封装 russh-sftp 客户端

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::models::sftp::{join_path, FileEntry, FileType};
use crate::ssh::session::SshSession;

/// 进度回调最小节流间隔（毫秒），防止大文件高速传输时每 256KB 触发一次 UI 重绘
const PROGRESS_CALLBACK_INTERVAL_MS: u128 = 100;

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
        debug!("[SFTP] Creating SFTP service for session {}", session_id);

        // 打开 SFTP 子系统通道
        let channel = ssh_session
            .handle()
            .read()
            .await
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

        debug!("[SFTP] SFTP service created for session {}", session_id);

        Ok(Self {
            session_id,
            sftp: Arc::new(sftp),
        })
    }

    /// 获取 SFTP 会话引用（用于并发操作）
    pub fn sftp(&self) -> Arc<SftpSession> {
        self.sftp.clone()
    }

    /// 检查路径（跟随符号链接）是否指向目录
    pub async fn is_dir_target(&self, path: &str) -> Result<bool, String> {
        debug!("[SFTP] Resolving target type for: {}", path);
        let attrs = self
            .sftp
            .metadata(path)
            .await
            .map_err(|e| format!("Failed to stat {}: {}", path, e))?;
        Ok(attrs.is_dir())
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

            let full_path = join_path(path, &name);
            let attrs = entry.metadata();

            // 确定文件类型
            let file_type = if attrs.is_dir() {
                FileType::Directory
            } else if attrs.is_symlink() {
                FileType::Symlink
            } else if attrs.is_regular() {
                FileType::File
            } else {
                FileType::Other
            };

            let mut file_entry = FileEntry::new(name.to_string(), full_path, file_type);
            file_entry.size = attrs.size.unwrap_or(0);
            file_entry.permissions = attrs.permissions.map(|p| p as u32).unwrap_or(0);
            file_entry.uid = attrs.uid;
            file_entry.gid = attrs.gid;

            // 修改时间
            if let Some(mtime) = attrs.mtime {
                file_entry.modified =
                    Some(std::time::UNIX_EPOCH + Duration::from_secs(mtime as u64));
            }

            entries.push(file_entry);
        }

        debug!("[SFTP] Read {} entries from {}", entries.len(), path);
        Ok(entries)
    }

    /// 创建目录
    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Creating directory: {}", path);
        self.sftp
            .create_dir(path)
            .await
            .map_err(|e| format!("Failed to create directory {}: {}", path, e))
    }

    /// 创建空文件
    pub async fn create_file(&self, path: &str) -> Result<(), String> {
        info!("[SFTP] Creating file: {}", path);
        use tokio::io::AsyncWriteExt;
        let mut file = self
            .sftp
            .create(path)
            .await
            .map_err(|e| format!("Failed to create file {}: {}", path, e))?;
        file.flush()
            .await
            .map_err(|e| format!("Failed to flush file {}: {}", path, e))?;
        Ok(())
    }

    /// 打开远程文件（用于读取）
    pub async fn open(&self, path: &str) -> Result<russh_sftp::client::fs::File, String> {
        debug!("[SFTP] Opening file: {}", path);
        self.sftp
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file {}: {}", path, e))
    }

    /// 写入文件内容
    pub async fn write_file(&self, path: &str, content: &[u8]) -> Result<(), String> {
        debug!("[SFTP] Writing file: {} ({} bytes)", path, content.len());
        use tokio::io::AsyncWriteExt;

        let mut file = self
            .sftp
            .create(path)
            .await
            .map_err(|e| format!("Failed to create file {}: {}", path, e))?;

        file.write_all(content)
            .await
            .map_err(|e| format!("Failed to write to file {}: {}", path, e))?;

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush file {}: {}", path, e))?;

        Ok(())
    }

    /// 读取符号链接目标
    pub async fn read_link(&self, path: &str) -> Result<String, String> {
        debug!("[SFTP] Reading symlink target: {}", path);
        self.sftp
            .read_link(path)
            .await
            .map_err(|e| format!("Failed to read link {}: {}", path, e))
    }

    /// 删除文件
    pub async fn remove_file(&self, path: &str) -> Result<(), String> {
        debug!("[SFTP] Removing file: {}", path);
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| format!("Failed to remove file {}: {}", path, e))
    }

    /// 删除目录（仅限空目录）
    pub async fn remove_dir(&self, path: &str) -> Result<(), String> {
        debug!("[SFTP] Removing directory: {}", path);
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| format!("Failed to remove directory {}: {}", path, e))
    }

    /// 递归删除目录（支持非空目录；符号链接仅移除链接本身，不跟随删除目标目录内容）
    pub async fn remove_dir_recursive(&self, path: &str) -> Result<(), String> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err("Refusing to recursively remove root directory '/'".to_string());
        }
        debug!("[SFTP] Removing directory recursively: {}", path);

        let entries = self.read_dir_recursive(path).await?;
        let (files, dirs) = plan_recursive_delete_order(path, &entries);

        for file_path in files {
            self.remove_file(&file_path).await?;
        }
        for dir_path in dirs {
            self.remove_dir(&dir_path).await?;
        }
        Ok(())
    }

    /// 重命名文件或目录
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        debug!("[SFTP] Renaming {} -> {}", from, to);
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| format!("Failed to rename {} to {}: {}", from, to, e))
    }

    /// 检查远程路径是否存在
    pub async fn exists(&self, path: &str) -> bool {
        self.sftp.metadata(path).await.is_ok()
    }

    /// 获取远程文件的 mtime（Unix 秒）
    pub async fn metadata_mtime(&self, path: &str) -> Option<u32> {
        self.sftp.metadata(path).await.ok().and_then(|m| m.mtime)
    }

    /// 修改远程文件/目录权限（chmod，支持 0o0000..=0o7777）
    pub async fn set_permissions(&self, path: &str, mode: u32) -> Result<(), String> {
        debug!("[SFTP] chmod {:o} {}", mode & 0o7777, path);
        let attrs = russh_sftp::protocol::FileAttributes {
            permissions: Some(mode & 0o7777),
            ..russh_sftp::protocol::FileAttributes::empty()
        };
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| format!("Failed to chmod {}: {}", path, e))
    }

    /// 设置远程文件的修改时间（Unix 秒）
    pub async fn set_mtime(&self, path: &str, mtime_secs: u32) -> Result<(), String> {
        let attrs = russh_sftp::protocol::FileAttributes {
            mtime: Some(mtime_secs),
            atime: Some(mtime_secs),
            ..russh_sftp::protocol::FileAttributes::empty()
        };
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| format!("Failed to set mtime for {}: {}", path, e))
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
        debug!("[SFTP] Reading directory recursively: {}", path);

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

        debug!(
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
        debug!("[SFTP] Creating directory recursively: {}", path);

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
        debug!(
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

        // 速度与节流变量
        let start_time = Instant::now();
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

            // 速度与进度回调统一按 100ms 节流，避免高频 UI 重绘
            let now = Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= PROGRESS_CALLBACK_INTERVAL_MS {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
                progress_callback(bytes_transferred, total_size, current_speed);
            }
        }

        // 确保所有数据都写入磁盘
        local_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush local file: {}", e))?;

        // 保证小文件与末帧 100% 进度必然通知到 UI
        if bytes_transferred != last_bytes || bytes_transferred == 0 {
            let total_elapsed = start_time.elapsed().as_secs_f64();
            if current_speed == 0 && total_elapsed > 0.0 {
                current_speed = (bytes_transferred as f64 / total_elapsed) as u64;
            }
            progress_callback(bytes_transferred, total_size, current_speed);
        }

        debug!(
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
    /// * `pause_flag` - 异步暂停标志
    /// * `cancel_token` - 取消令牌
    /// * `progress_callback` - 进度回调函数，参数为 (本分片已传输字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(bytes_downloaded)` - 下载成功，返回实际下载的字节数
    /// * `Err(String)` - 下载失败，包含错误信息
    #[allow(clippy::too_many_arguments)]
    pub async fn download_chunk<F>(
        &self,
        remote_path: &str,
        local_file: Arc<tokio::sync::Mutex<tokio::fs::File>>,
        offset: u64,
        length: u64,
        pause_flag: &Arc<AtomicBool>,
        cancel_token: &CancellationToken,
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

        // 速度与节流变量
        let start_time = Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        while remaining > 0 {
            // 异步暂停等待，避免阻塞 Tokio worker 线程
            while pause_flag.load(Ordering::Relaxed) {
                if cancel_token.is_cancelled() {
                    return Err("下载已取消".to_string());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                last_update_time = Instant::now();
                last_bytes = bytes_transferred;
            }
            if cancel_token.is_cancelled() {
                return Err("下载已取消".to_string());
            }

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

            // 按 100ms 节流计算速度并通知分片进度
            let now = Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= PROGRESS_CALLBACK_INTERVAL_MS {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
                progress_callback(bytes_transferred, current_speed);
            }
        }

        // 确保本分片最终字节数上报
        if bytes_transferred != last_bytes {
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
        debug!("[SFTP] Uploading file: {:?} -> {}", local_path, remote_path);

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

        // 速度与节流变量
        let start_time = Instant::now();
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

            // 速度与进度回调统一按 100ms 节流，避免高频 UI 重绘
            let now = Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= PROGRESS_CALLBACK_INTERVAL_MS {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
                progress_callback(bytes_transferred, total_size, current_speed);
            }
        }

        // 确保所有数据都写入远程
        remote_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush remote file: {}", e))?;

        // 保证小文件与末帧 100% 进度必然通知到 UI
        if bytes_transferred != last_bytes || bytes_transferred == 0 {
            let total_elapsed = start_time.elapsed().as_secs_f64();
            if current_speed == 0 && total_elapsed > 0.0 {
                current_speed = (bytes_transferred as f64 / total_elapsed) as u64;
            }
            progress_callback(bytes_transferred, total_size, current_speed);
        }

        debug!(
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
    /// * `pause_flag` - 异步暂停标志
    /// * `cancel_token` - 取消令牌
    /// * `progress_callback` - 进度回调函数，参数为 (本分片已传输字节数, 速度bytes/s)
    ///
    /// # Returns
    /// * `Ok(bytes_uploaded)` - 上传成功，返回实际上传的字节数
    /// * `Err(String)` - 上传失败，包含错误信息
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_chunk<F>(
        &self,
        local_file: Arc<tokio::sync::Mutex<tokio::fs::File>>,
        remote_path: &str,
        offset: u64,
        length: u64,
        pause_flag: &Arc<AtomicBool>,
        cancel_token: &CancellationToken,
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

        // 速度与节流变量
        let start_time = Instant::now();
        let mut last_update_time = start_time;
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;

        while remaining > 0 {
            // 异步暂停等待，避免阻塞 Tokio worker 线程
            while pause_flag.load(Ordering::Relaxed) {
                if cancel_token.is_cancelled() {
                    return Err("上传已取消".to_string());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                last_update_time = Instant::now();
                last_bytes = bytes_transferred;
            }
            if cancel_token.is_cancelled() {
                return Err("上传已取消".to_string());
            }

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

            // 按 100ms 节流计算速度并通知分片进度
            let now = Instant::now();
            let elapsed = now.duration_since(last_update_time);
            if elapsed.as_millis() >= PROGRESS_CALLBACK_INTERVAL_MS {
                let bytes_delta = bytes_transferred - last_bytes;
                current_speed = (bytes_delta as f64 / elapsed.as_secs_f64()) as u64;
                last_update_time = now;
                last_bytes = bytes_transferred;
                progress_callback(bytes_transferred, current_speed);
            }
        }

        // 确保数据写入
        remote_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush remote file: {}", e))?;

        // 确保本分片最终字节数上报
        if bytes_transferred != last_bytes {
            progress_callback(bytes_transferred, current_speed);
        }

        debug!(
            "[SFTP] Chunk upload completed: {} offset={} uploaded={}",
            remote_path, offset, bytes_transferred
        );

        Ok(bytes_transferred)
    }
}

impl Drop for SftpService {
    fn drop(&mut self) {
        debug!(
            "[SFTP] Dropping SFTP service for session {}",
            self.session_id
        );
    }
}

/// 计算递归删除目录时的安全删除顺序：(先删除的非目录文件/符号链接, 后按深度降序删除的目录列表)
pub(crate) fn plan_recursive_delete_order(
    root: &str,
    entries: &[FileEntry],
) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in entries {
        // 仅真实目录视为子目录；符号链接按文件直接 unlink，避免穿透删除链接目标目录
        if entry.is_dir() {
            dirs.push(entry.path.clone());
        } else {
            files.push(entry.path.clone());
        }
    }
    dirs.sort_by(|a, b| {
        let depth_a = a.matches('/').count();
        let depth_b = b.matches('/').count();
        depth_b
            .cmp(&depth_a)
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| a.cmp(b))
    });
    dirs.push(root.to_string());
    (files, dirs)
}

/// 解析用户输入的八进制权限字符串（如 "755"、"0644"、"4755"）
pub fn parse_octal_permissions(input: &str) -> Option<u32> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 4 || !trimmed.chars().all(|c| ('0'..='7').contains(&c)) {
        return None;
    }
    u32::from_str_radix(trimmed, 8).ok().filter(|&m| m <= 0o7777)
}

/// 生成文件冲突时的第 `index`（从 1 开始）个重命名候选名，如 `notes.txt` -> `notes (1).txt`
pub fn renamed_conflict_name(file_name: &str, index: usize) -> String {
    if let Some((stem, ext)) = file_name.rsplit_once('.') {
        if !stem.is_empty() {
            return format!("{stem} ({index}).{ext}");
        }
    }
    format!("{file_name} ({index})")
}

#[cfg(test)]
mod tests {
    use super::{parse_octal_permissions, plan_recursive_delete_order, renamed_conflict_name};
    use crate::models::sftp::{FileEntry, FileType};

    #[test]
    fn octal_permissions_and_conflict_renaming() {
        assert_eq!(parse_octal_permissions("755"), Some(0o755));
        assert_eq!(parse_octal_permissions("0644"), Some(0o644));
        assert_eq!(parse_octal_permissions("4755"), Some(0o4755));
        assert_eq!(parse_octal_permissions("888"), None);
        assert_eq!(parse_octal_permissions(""), None);
        assert_eq!(parse_octal_permissions("12345"), None);

        assert_eq!(renamed_conflict_name("api.conf", 1), "api (1).conf");
        assert_eq!(renamed_conflict_name(".env", 2), ".env (2)");
        assert_eq!(renamed_conflict_name("Makefile", 1), "Makefile (1)");
    }

    #[test]
    fn recursive_delete_plan_removes_files_and_symlinks_before_deepest_dirs() {
        let entries = vec![
            FileEntry::new("sub".into(), "/tmp/app/sub".into(), FileType::Directory),
            FileEntry::new(
                "nested".into(),
                "/tmp/app/sub/nested".into(),
                FileType::Directory,
            ),
            FileEntry::new(
                "a.txt".into(),
                "/tmp/app/sub/nested/a.txt".into(),
                FileType::File,
            ),
            FileEntry::new(
                "link_to_etc".into(),
                "/tmp/app/link_to_etc".into(),
                FileType::Symlink,
            ),
        ];

        let (files, dirs) = plan_recursive_delete_order("/tmp/app", &entries);
        assert_eq!(
            files,
            vec!["/tmp/app/sub/nested/a.txt", "/tmp/app/link_to_etc"]
        );
        assert_eq!(dirs, vec!["/tmp/app/sub/nested", "/tmp/app/sub", "/tmp/app"]);
    }
}
