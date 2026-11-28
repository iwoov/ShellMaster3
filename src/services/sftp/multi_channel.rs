// 多通道并行下载/上传器
// 将文件分成多个分片，使用多个 SFTP 通道并行传输

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::service::SftpService;
use crate::ssh::session::SshSession;

/// 多通道聚合进度节流间隔（毫秒）
const AGGREGATED_PROGRESS_INTERVAL_MS: u128 = 100;

/// 多通道下载器
/// 将文件分成多个分片，使用多个 SFTP 通道并行下载
pub struct MultiChannelDownloader {
    /// SSH 会话
    ssh_session: Arc<SshSession>,
    /// 会话 ID
    session_id: String,
    /// 并行通道数
    channel_count: usize,
}

/// 分片传输任务
#[derive(Debug, Clone)]
struct ChunkTask {
    /// 分片索引
    index: usize,
    /// 起始偏移量
    offset: u64,
    /// 分片长度
    length: u64,
}

/// 分片传输进度
#[derive(Debug, Clone, Default)]
struct ChunkProgress {
    /// 已传输字节数
    bytes_transferred: u64,
    /// 当前速度
    speed: u64,
}

/// 多通道进度聚合器（同步短锁，避免在异步回调中 block_in_place）
struct ProgressAggregator {
    chunks: Vec<ChunkProgress>,
    last_emit: Instant,
    has_emitted: bool,
}

impl ProgressAggregator {
    fn new(task_count: usize) -> Self {
        Self {
            chunks: vec![ChunkProgress::default(); task_count],
            last_emit: Instant::now(),
            has_emitted: false,
        }
    }

    /// 更新指定分片进度，若满足节流条件或已全部完成则返回 (总字节, 总速度)
    fn update(
        &mut self,
        chunk_idx: usize,
        bytes_transferred: u64,
        speed: u64,
        file_size: u64,
    ) -> Option<(u64, u64)> {
        if let Some(slot) = self.chunks.get_mut(chunk_idx) {
            slot.bytes_transferred = bytes_transferred;
            slot.speed = speed;
        }
        let total_transferred: u64 = self.chunks.iter().map(|p| p.bytes_transferred).sum();
        let total_speed: u64 = self.chunks.iter().map(|p| p.speed).sum();

        let now = Instant::now();
        let should_emit = !self.has_emitted
            || total_transferred >= file_size
            || now.duration_since(self.last_emit).as_millis() >= AGGREGATED_PROGRESS_INTERVAL_MS;

        if should_emit {
            self.last_emit = now;
            self.has_emitted = true;
            Some((total_transferred, total_speed))
        } else {
            None
        }
    }

    fn totals(&self) -> (u64, u64) {
        let total_transferred: u64 = self.chunks.iter().map(|p| p.bytes_transferred).sum();
        let total_speed: u64 = self.chunks.iter().map(|p| p.speed).sum();
        (total_transferred, total_speed)
    }
}

impl MultiChannelDownloader {
    /// 创建多通道下载器
    ///
    /// # Arguments
    /// * `ssh_session` - SSH 会话
    /// * `session_id` - 会话 ID
    /// * `channel_count` - 并行通道数（建议 2-8）
    pub fn new(ssh_session: Arc<SshSession>, session_id: String, channel_count: usize) -> Self {
        // 限制通道数在合理范围内
        let channel_count = channel_count.clamp(1, 8);
        Self {
            ssh_session,
            session_id,
            channel_count,
        }
    }

    /// 并行下载文件
    ///
    /// # Arguments
    /// * `remote_path` - 远程文件路径
    /// * `local_path` - 本地保存路径
    /// * `file_size` - 文件大小
    /// * `cancel_token` - 取消令牌
    /// * `pause_flag` - 暂停标志
    /// * `progress_callback` - 进度回调函数，参数为 (总已传输字节数, 总字节数, 总速度)
    ///
    /// # Returns
    /// * `Ok(())` - 下载成功
    /// * `Err(String)` - 下载失败
    pub async fn download_file<F>(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
        file_size: u64,
        cancel_token: CancellationToken,
        pause_flag: Arc<std::sync::atomic::AtomicBool>,
        progress_callback: F,
    ) -> Result<(), String>
    where
        F: Fn(u64, u64, u64) + Send + Sync + 'static,
    {
        info!(
            "[SFTP] Multi-channel download: {} ({} bytes) with {} channels",
            remote_path, file_size, self.channel_count
        );

        // 创建分片任务
        let tasks = self.create_chunk_tasks(file_size);
        let task_count = tasks.len();

        info!("[SFTP] Created {} chunk tasks", task_count);

        // 创建本地文件（预分配空间）
        let local_file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(local_path)
            .await
            .map_err(|e| format!("Failed to create local file: {}", e))?;

        // 预分配文件大小
        local_file
            .set_len(file_size)
            .await
            .map_err(|e| format!("Failed to set file size: {}", e))?;

        let local_file = Arc::new(AsyncMutex::new(local_file));

        // 创建无阻塞进度聚合器
        let progress_tracker = Arc::new(StdMutex::new(ProgressAggregator::new(task_count)));

        // 包装回调函数
        let progress_callback = Arc::new(progress_callback);

        // 启动并行下载任务
        let mut handles = Vec::with_capacity(task_count);

        for task in tasks {
            let ssh_session = self.ssh_session.clone();
            let session_id = self.session_id.clone();
            let remote_path = remote_path.to_string();
            let local_file = local_file.clone();
            let progress_tracker = progress_tracker.clone();
            let progress_callback = progress_callback.clone();
            let pause_flag_clone = pause_flag.clone();
            let cancel_token_clone = cancel_token.clone();

            let handle = tokio::spawn(async move {
                // 为每个分片创建独立的 SFTP 服务
                let sftp_service = match SftpService::new(
                    format!("{}-chunk-{}", session_id, task.index),
                    &ssh_session,
                )
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "[SFTP] Failed to create SFTP channel for chunk {}: {}",
                            task.index, e
                        );
                        return Err(e);
                    }
                };

                let chunk_idx = task.index;

                // 下载分片：暂停和取消在异步循环内部非阻塞处理，进度更新走轻量同步短锁
                let result = sftp_service
                    .download_chunk(
                        &remote_path,
                        local_file,
                        task.offset,
                        task.length,
                        &pause_flag_clone,
                        &cancel_token_clone,
                        move |bytes_transferred, speed| {
                            let emitted = progress_tracker.lock().ok().and_then(|mut agg| {
                                agg.update(chunk_idx, bytes_transferred, speed, file_size)
                            });
                            if let Some((total_transferred, total_speed)) = emitted {
                                progress_callback(total_transferred, file_size, total_speed);
                            }
                        },
                    )
                    .await;

                match result {
                    Ok(bytes) => {
                        debug!("[SFTP] Chunk {} completed: {} bytes", task.index, bytes);
                        Ok(bytes)
                    }
                    Err(e) => {
                        error!("[SFTP] Chunk {} failed: {}", task.index, e);
                        Err(e)
                    }
                }
            });

            handles.push(handle);
        }

        // 等待所有任务完成，同时监听取消信号
        let local_path_for_cleanup = local_path.to_path_buf();

        // 使用 futures::future::join_all 来并行等待所有任务
        let all_tasks = futures::future::join_all(handles);

        let results = tokio::select! {
            // 监听取消信号
            _ = cancel_token.cancelled() => {
                warn!("[SFTP] Multi-channel download cancelled by user");
                // 删除不完整的文件
                let _ = tokio::fs::remove_file(&local_path_for_cleanup).await;
                return Err("下载已取消".to_string());
            }
            // 等待所有下载任务完成
            results = all_tasks => results,
        };

        // 处理结果
        let mut total_bytes = 0u64;
        let mut errors = Vec::new();

        for (idx, result) in results.into_iter().enumerate() {
            match result {
                Ok(Ok(bytes)) => {
                    total_bytes += bytes;
                }
                Ok(Err(e)) => {
                    errors.push(format!("Chunk {}: {}", idx, e));
                }
                Err(e) => {
                    errors.push(format!("Chunk {} task panic: {}", idx, e));
                }
            }
        }

        // 检查是否有错误
        if !errors.is_empty() {
            // 删除不完整的文件
            let _ = tokio::fs::remove_file(local_path).await;
            return Err(format!(
                "Multi-channel download failed:\n{}",
                errors.join("\n")
            ));
        }

        // 确保文件写入完成
        {
            let file = local_file.lock().await;
            file.sync_all()
                .await
                .map_err(|e| format!("Failed to sync file: {}", e))?;
        }

        if let Ok(agg) = progress_tracker.lock() {
            let (transferred, speed) = agg.totals();
            progress_callback(transferred.max(total_bytes), file_size, speed);
        }

        info!(
            "[SFTP] Multi-channel download completed: {} ({} bytes)",
            remote_path, total_bytes
        );

        Ok(())
    }

    /// 创建分片任务
    fn create_chunk_tasks(&self, file_size: u64) -> Vec<ChunkTask> {
        // 最小分片大小为 1MB
        const MIN_CHUNK_SIZE: u64 = 1024 * 1024;

        // 计算每个分片的大小
        let chunk_size = std::cmp::max(
            MIN_CHUNK_SIZE,
            file_size.div_ceil(self.channel_count as u64),
        );

        let mut tasks = Vec::new();
        let mut offset = 0u64;
        let mut index = 0;

        while offset < file_size {
            let length = std::cmp::min(chunk_size, file_size - offset);
            tasks.push(ChunkTask {
                index,
                offset,
                length,
            });
            offset += length;
            index += 1;
        }

        tasks
    }
}

/// 多通道上传器
/// 将文件分成多个分片，使用多个 SFTP 通道并行上传
pub struct MultiChannelUploader {
    /// SSH 会话
    ssh_session: Arc<SshSession>,
    /// 会话 ID
    session_id: String,
    /// 并行通道数
    channel_count: usize,
}

impl MultiChannelUploader {
    /// 创建多通道上传器
    ///
    /// # Arguments
    /// * `ssh_session` - SSH 会话
    /// * `session_id` - 会话 ID
    /// * `channel_count` - 并行通道数（建议 2-8）
    pub fn new(ssh_session: Arc<SshSession>, session_id: String, channel_count: usize) -> Self {
        // 限制通道数在合理范围内
        let channel_count = channel_count.clamp(1, 8);
        Self {
            ssh_session,
            session_id,
            channel_count,
        }
    }

    /// 并行上传文件
    ///
    /// # Arguments
    /// * `local_path` - 本地文件路径
    /// * `remote_path` - 远程保存路径
    /// * `file_size` - 文件大小
    /// * `cancel_token` - 取消令牌
    /// * `pause_flag` - 暂停标志
    /// * `progress_callback` - 进度回调函数，参数为 (总已传输字节数, 总字节数, 总速度)
    ///
    /// # Returns
    /// * `Ok(())` - 上传成功
    /// * `Err(String)` - 上传失败
    pub async fn upload_file<F>(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
        file_size: u64,
        cancel_token: CancellationToken,
        pause_flag: Arc<std::sync::atomic::AtomicBool>,
        progress_callback: F,
    ) -> Result<(), String>
    where
        F: Fn(u64, u64, u64) + Send + Sync + 'static,
    {
        info!(
            "[SFTP] Multi-channel upload: {:?} -> {} ({} bytes) with {} channels",
            local_path, remote_path, file_size, self.channel_count
        );

        // 创建分片任务
        let tasks = self.create_chunk_tasks(file_size);
        let task_count = tasks.len();

        info!("[SFTP] Created {} upload chunk tasks", task_count);

        // 打开本地文件
        let local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| format!("Failed to open local file: {}", e))?;
        let local_file = Arc::new(AsyncMutex::new(local_file));

        // 在远程创建空文件（使用主 SFTP 服务）
        let main_sftp = SftpService::new(
            format!("{}-upload-main", self.session_id),
            &self.ssh_session,
        )
        .await
        .map_err(|e| format!("Failed to create main SFTP channel: {}", e))?;

        // 创建远程文件
        let remote_file = main_sftp
            .sftp()
            .create(remote_path)
            .await
            .map_err(|e| format!("Failed to create remote file: {}", e))?;

        // 关闭文件，让分片任务可以写入
        drop(remote_file);

        // 创建无阻塞进度聚合器
        let progress_tracker = Arc::new(StdMutex::new(ProgressAggregator::new(task_count)));

        // 包装回调函数
        let progress_callback = Arc::new(progress_callback);

        // 启动并行上传任务
        let mut handles = Vec::with_capacity(task_count);

        for task in tasks {
            let ssh_session = self.ssh_session.clone();
            let session_id = self.session_id.clone();
            let remote_path = remote_path.to_string();
            let local_file = local_file.clone();
            let progress_tracker = progress_tracker.clone();
            let progress_callback = progress_callback.clone();
            let pause_flag_clone = pause_flag.clone();
            let cancel_token_clone = cancel_token.clone();

            let handle = tokio::spawn(async move {
                // 为每个分片创建独立的 SFTP 服务
                let sftp_service = match SftpService::new(
                    format!("{}-upload-chunk-{}", session_id, task.index),
                    &ssh_session,
                )
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "[SFTP] Failed to create SFTP channel for upload chunk {}: {}",
                            task.index, e
                        );
                        return Err(e);
                    }
                };

                let chunk_idx = task.index;

                // 上传分片：暂停和取消在异步循环内部非阻塞处理，进度更新走轻量同步短锁
                let result = sftp_service
                    .upload_chunk(
                        local_file,
                        &remote_path,
                        task.offset,
                        task.length,
                        &pause_flag_clone,
                        &cancel_token_clone,
                        move |bytes_transferred, speed| {
                            let emitted = progress_tracker.lock().ok().and_then(|mut agg| {
                                agg.update(chunk_idx, bytes_transferred, speed, file_size)
                            });
                            if let Some((total_transferred, total_speed)) = emitted {
                                progress_callback(total_transferred, file_size, total_speed);
                            }
                        },
                    )
                    .await;

                match result {
                    Ok(bytes) => {
                        debug!(
                            "[SFTP] Upload chunk {} completed: {} bytes",
                            task.index, bytes
                        );
                        Ok(bytes)
                    }
                    Err(e) => {
                        error!("[SFTP] Upload chunk {} failed: {}", task.index, e);
                        Err(e)
                    }
                }
            });

            handles.push(handle);
        }

        // 等待所有任务完成，同时监听取消信号
        let remote_path_for_cleanup = remote_path.to_string();

        // 使用 futures::future::join_all 来并行等待所有任务
        let all_tasks = futures::future::join_all(handles);

        let results = tokio::select! {
            // 监听取消信号
            _ = cancel_token.cancelled() => {
                warn!("[SFTP] Multi-channel upload cancelled by user");
                // 删除不完整的远程文件
                let _ = main_sftp.remove_file(&remote_path_for_cleanup).await;
                return Err("上传已取消".to_string());
            }
            // 等待所有上传任务完成
            results = all_tasks => results,
        };

        // 处理结果
        let mut total_bytes = 0u64;
        let mut errors = Vec::new();

        for (idx, result) in results.into_iter().enumerate() {
            match result {
                Ok(Ok(bytes)) => {
                    total_bytes += bytes;
                }
                Ok(Err(e)) => {
                    errors.push(format!("Chunk {}: {}", idx, e));
                }
                Err(e) => {
                    errors.push(format!("Chunk {} task panic: {}", idx, e));
                }
            }
        }

        // 检查是否有错误
        if !errors.is_empty() {
            // 删除不完整的远程文件
            let _ = main_sftp.remove_file(&remote_path_for_cleanup).await;
            return Err(format!(
                "Multi-channel upload failed:\n{}",
                errors.join("\n")
            ));
        }

        if let Ok(agg) = progress_tracker.lock() {
            let (transferred, speed) = agg.totals();
            progress_callback(transferred.max(total_bytes), file_size, speed);
        }

        info!(
            "[SFTP] Multi-channel upload completed: {} ({} bytes)",
            remote_path, total_bytes
        );

        Ok(())
    }

    /// 创建分片任务
    fn create_chunk_tasks(&self, file_size: u64) -> Vec<ChunkTask> {
        // 最小分片大小为 1MB
        const MIN_CHUNK_SIZE: u64 = 1024 * 1024;

        // 计算每个分片的大小
        let chunk_size = std::cmp::max(
            MIN_CHUNK_SIZE,
            file_size.div_ceil(self.channel_count as u64),
        );

        let mut tasks = Vec::new();
        let mut offset = 0u64;
        let mut index = 0;

        while offset < file_size {
            let length = std::cmp::min(chunk_size, file_size - offset);
            tasks.push(ChunkTask {
                index,
                offset,
                length,
            });
            offset += length;
            index += 1;
        }

        tasks
    }
}
