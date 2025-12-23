// SFTP 传输相关类型
// 定义传输状态、进度和传输项

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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

    /// 是否是活动状态（可以接收进度更新）
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TransferStatus::Pending
                | TransferStatus::Downloading
                | TransferStatus::Uploading
                | TransferStatus::Paused
        )
    }

    /// 是否是终态（不能再变更）
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
        )
    }

    /// 检查是否可以转换到指定状态
    pub fn can_transition_to(&self, next: &TransferStatus) -> bool {
        // 终态不能转换到任何其他状态
        if self.is_terminal() {
            return false;
        }

        match (self, next) {
            // Pending 可以转换到开始状态
            (TransferStatus::Pending, TransferStatus::Downloading) => true,
            (TransferStatus::Pending, TransferStatus::Uploading) => true,
            (TransferStatus::Pending, TransferStatus::Cancelled) => true,

            // Downloading 可以暂停、完成、失败、取消
            (TransferStatus::Downloading, TransferStatus::Paused) => true,
            (TransferStatus::Downloading, TransferStatus::Completed) => true,
            (TransferStatus::Downloading, TransferStatus::Failed) => true,
            (TransferStatus::Downloading, TransferStatus::Cancelled) => true,

            // Uploading 可以暂停、完成、失败、取消
            (TransferStatus::Uploading, TransferStatus::Paused) => true,
            (TransferStatus::Uploading, TransferStatus::Completed) => true,
            (TransferStatus::Uploading, TransferStatus::Failed) => true,
            (TransferStatus::Uploading, TransferStatus::Cancelled) => true,

            // Paused 可以恢复或取消
            (TransferStatus::Paused, TransferStatus::Downloading) => true,
            (TransferStatus::Paused, TransferStatus::Uploading) => true,
            (TransferStatus::Paused, TransferStatus::Cancelled) => true,

            _ => false,
        }
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
}

impl TransferProgress {
    /// 创建新的传输进度
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_transferred: 0,
            total_bytes,
            speed_bytes_per_sec: 0,
        }
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
    /// 暂停标志
    pub pause_flag: Arc<AtomicBool>,
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
            pause_flag: Arc::new(AtomicBool::new(false)),
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
            pause_flag: Arc::new(AtomicBool::new(false)),
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

    /// 更新进度（不改变状态）
    /// 这是安全的进度更新方法，只更新进度数据，不修改状态
    pub fn update_progress(&mut self, transferred: u64, total: u64, speed: u64) {
        // 只在活动状态时更新进度
        if !self.status.is_active() {
            return;
        }

        self.progress.bytes_transferred = transferred;
        self.progress.total_bytes = total;

        // 暂停时不更新速度（保持为0）
        if self.status != TransferStatus::Paused {
            self.progress.speed_bytes_per_sec = speed;

            // 如果是 Pending 状态，自动切换到 Downloading
            if self.status == TransferStatus::Pending {
                self.status = TransferStatus::Downloading;
            }
        }
    }

    /// 暂停传输
    pub fn pause(&mut self) -> bool {
        use std::sync::atomic::Ordering;

        if self.status.can_transition_to(&TransferStatus::Paused) {
            self.status = TransferStatus::Paused;
            self.pause_flag.store(true, Ordering::Relaxed);
            self.progress.speed_bytes_per_sec = 0; // 暂停时速度归零
            true
        } else {
            false
        }
    }

    /// 恢复传输
    pub fn resume(&mut self) -> bool {
        use std::sync::atomic::Ordering;

        let next_status = if self.is_upload {
            TransferStatus::Uploading
        } else {
            TransferStatus::Downloading
        };

        if self.status.can_transition_to(&next_status) {
            self.status = next_status;
            self.pause_flag.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
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
