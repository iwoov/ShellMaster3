// SFTP 传输相关类型
// 定义传输状态、进度和传输项

use std::path::PathBuf;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// 传输状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    /// 等待中
    Pending,
    /// 下载中
    Downloading,
    /// 上传中
    Uploading,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

impl Default for TransferStatus {
    fn default() -> Self {
        TransferStatus::Pending
    }
}

impl TransferStatus {
    /// 获取状态的显示文本
    pub fn display_text(&self) -> &'static str {
        match self {
            TransferStatus::Pending => "等待中",
            TransferStatus::Downloading => "下载中",
            TransferStatus::Uploading => "上传中",
            TransferStatus::Paused => "已暂停",
            TransferStatus::Completed => "完成",
            TransferStatus::Failed => "失败",
            TransferStatus::Cancelled => "已取消",
        }
    }

    /// 是否是错误状态
    pub fn is_error(&self) -> bool {
        matches!(self, TransferStatus::Failed | TransferStatus::Cancelled)
    }

    /// 是否已完成
    pub fn is_complete(&self) -> bool {
        matches!(self, TransferStatus::Completed)
    }
}

/// 传输进度
#[derive(Debug, Clone, Default)]
pub struct TransferProgress {
    /// 已传输字节数
    pub bytes_transferred: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 传输速度 (bytes/s)
    pub speed_bytes_per_sec: u64,
    /// 开始时间
    pub started_at: Option<Instant>,
}

impl TransferProgress {
    /// 创建新的传输进度
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_transferred: 0,
            total_bytes,
            speed_bytes_per_sec: 0,
            started_at: None,
        }
    }

    /// 更新进度
    pub fn update(&mut self, bytes_transferred: u64) {
        self.bytes_transferred = bytes_transferred;

        // 计算速度
        if let Some(started_at) = self.started_at {
            let elapsed = started_at.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                self.speed_bytes_per_sec = (bytes_transferred as f64 / elapsed) as u64;
            }
        }
    }

    /// 开始传输
    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
    }

    /// 完成传输
    pub fn complete(&mut self) {
        self.bytes_transferred = self.total_bytes;
    }

    /// 获取进度百分比 (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.bytes_transferred as f64 / self.total_bytes as f64) * 100.0
    }

    /// 格式化速度显示
    pub fn format_speed(&self) -> String {
        format_bytes_per_sec(self.speed_bytes_per_sec)
    }

    /// 预估剩余时间（秒）
    pub fn estimated_remaining_secs(&self) -> Option<u64> {
        if self.speed_bytes_per_sec == 0 {
            return None;
        }
        let remaining_bytes = self.total_bytes.saturating_sub(self.bytes_transferred);
        Some(remaining_bytes / self.speed_bytes_per_sec)
    }
}

/// 传输项
#[derive(Clone)]
pub struct TransferItem {
    /// 唯一标识符
    pub id: String,
    /// 远程文件路径
    pub remote_path: String,
    /// 本地文件路径
    pub local_path: PathBuf,
    /// 传输状态
    pub status: TransferStatus,
    /// 传输进度
    pub progress: TransferProgress,
    /// 错误信息
    pub error: Option<String>,
    /// 是否是上传（false 表示下载）
    pub is_upload: bool,
    /// 取消令牌
    pub cancel_token: CancellationToken,
}

impl TransferItem {
    /// 创建新的下载项
    pub fn new_download(remote_path: String, local_path: PathBuf, total_bytes: u64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            remote_path,
            local_path,
            status: TransferStatus::Pending,
            progress: TransferProgress::new(total_bytes),
            error: None,
            is_upload: false,
            cancel_token: CancellationToken::new(),
        }
    }

    /// 创建新的上传项
    pub fn new_upload(local_path: PathBuf, remote_path: String, total_bytes: u64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            remote_path,
            local_path,
            status: TransferStatus::Pending,
            progress: TransferProgress::new(total_bytes),
            error: None,
            is_upload: true,
            cancel_token: CancellationToken::new(),
        }
    }

    /// 设置失败状态
    pub fn set_failed(&mut self, error: String) {
        self.status = TransferStatus::Failed;
        self.error = Some(error);
    }

    /// 设置完成状态
    pub fn set_completed(&mut self) {
        self.status = TransferStatus::Completed;
        self.progress.complete();
    }

    /// 获取文件名
    pub fn file_name(&self) -> String {
        if self.is_upload {
            self.local_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.local_path.to_string_lossy().to_string())
        } else {
            self.remote_path
                .rsplit('/')
                .next()
                .unwrap_or(&self.remote_path)
                .to_string()
        }
    }
}

/// 格式化字节速度
fn format_bytes_per_sec(bytes_per_sec: u64) -> String {
    let speed = bytes_per_sec as f64;
    if speed >= 1_073_741_824.0 {
        format!("{:.1} GB/s", speed / 1_073_741_824.0)
    } else if speed >= 1_048_576.0 {
        format!("{:.1} MB/s", speed / 1_048_576.0)
    } else if speed >= 1_024.0 {
        format!("{:.1} KB/s", speed / 1_024.0)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}
