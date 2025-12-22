// 多通道并行下载器
// 将文件分成多个分片，使用多个 SFTP 通道并行下载

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use super::service::SftpService;
use crate::ssh::session::SshSession;

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

/// 分片下载任务
#[derive(Debug, Clone)]
struct ChunkTask {
    /// 分片索引
    index: usize,
    /// 起始偏移量
    offset: u64,
    /// 分片长度
    length: u64,
}

/// 分片下载进度
#[derive(Debug, Clone, Default)]
struct ChunkProgress {
    /// 已传输字节数
    bytes_transferred: u64,
    /// 当前速度
    speed: u64,
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

        let local_file = Arc::new(Mutex::new(local_file));

        // 创建进度追踪
        let progress_tracker = Arc::new(Mutex::new(vec![ChunkProgress::default(); task_count]));

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
            let file_size = file_size;

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

                // 下载分片
                let result = sftp_service
                    .download_chunk(
                        &remote_path,
                        local_file,
                        task.offset,
                        task.length,
                        move |bytes_transferred, speed| {
                            // 更新分片进度
                            let progress_tracker = progress_tracker.clone();
                            let progress_callback = progress_callback.clone();

                            // 使用 block_in_place 在同步上下文中更新
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    let mut tracker = progress_tracker.lock().await;
                                    tracker[chunk_idx].bytes_transferred = bytes_transferred;
                                    tracker[chunk_idx].speed = speed;

                                    // 计算总进度
                                    let total_transferred: u64 =
                                        tracker.iter().map(|p| p.bytes_transferred).sum();
                                    let total_speed: u64 = tracker.iter().map(|p| p.speed).sum();

                                    // 调用总进度回调
                                    progress_callback(total_transferred, file_size, total_speed);
                                });
                            });
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

        // 等待所有任务完成
        let mut total_bytes = 0u64;
        let mut errors = Vec::new();

        for (idx, handle) in handles.into_iter().enumerate() {
            match handle.await {
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
            let mut file = local_file.lock().await;
            file.sync_all()
                .await
                .map_err(|e| format!("Failed to sync file: {}", e))?;
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
            (file_size + self.channel_count as u64 - 1) / self.channel_count as u64,
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

use tokio::io::AsyncWriteExt;
