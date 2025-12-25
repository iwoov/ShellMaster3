// 属性对话框状态管理

use tokio_util::sync::CancellationToken;

use crate::models::sftp::FileEntry;

/// 属性对话框状态
pub struct PropertiesDialogState {
    /// 是否打开
    pub is_open: bool,
    /// 文件/文件夹条目信息
    pub entry: Option<FileEntry>,
    /// 符号链接目标（仅对符号链接有效）
    pub symlink_target: Option<String>,
    /// 文件夹大小（仅对文件夹有效，需要异步计算）
    pub folder_size: Option<u64>,
    /// 是否正在计算文件夹大小
    pub is_calculating_size: bool,
    /// 关联的 tab_id
    pub tab_id: String,
    /// 取消计算的 token
    cancellation_token: Option<CancellationToken>,
}

impl Default for PropertiesDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            entry: None,
            symlink_target: None,
            folder_size: None,
            is_calculating_size: false,
            tab_id: String::new(),
            cancellation_token: None,
        }
    }
}

impl PropertiesDialogState {
    /// 打开对话框
    pub fn open(&mut self, entry: FileEntry, tab_id: String) {
        // 取消之前的计算（如果有）
        self.cancel_calculation();

        self.is_open = true;
        self.entry = Some(entry);
        self.symlink_target = None;
        self.folder_size = None;
        self.is_calculating_size = false;
        self.tab_id = tab_id;
    }

    /// 关闭对话框
    pub fn close(&mut self) {
        // 取消正在进行的计算
        self.cancel_calculation();

        self.is_open = false;
        self.entry = None;
        self.symlink_target = None;
        self.folder_size = None;
        self.is_calculating_size = false;
        self.tab_id.clear();
    }

    /// 取消计算
    fn cancel_calculation(&mut self) {
        if let Some(token) = self.cancellation_token.take() {
            token.cancel();
        }
    }

    /// 设置符号链接目标
    pub fn set_symlink_target(&mut self, target: String) {
        self.symlink_target = Some(target);
    }

    /// 开始计算文件夹大小，返回 CancellationToken 供异步任务使用
    pub fn start_calculating_size(&mut self) -> CancellationToken {
        // 取消之前的计算（如果有）
        self.cancel_calculation();

        let token = CancellationToken::new();
        self.cancellation_token = Some(token.clone());
        self.is_calculating_size = true;
        token
    }

    /// 设置文件夹大小（计算完成）
    pub fn set_folder_size(&mut self, size: u64) {
        self.folder_size = Some(size);
        self.is_calculating_size = false;
        self.cancellation_token = None;
    }

    /// 格式化文件夹大小
    pub fn format_folder_size(&self) -> String {
        if let Some(size) = self.folder_size {
            let size_f = size as f64;
            if size_f >= 1_073_741_824.0 {
                format!("{:.2} GB", size_f / 1_073_741_824.0)
            } else if size_f >= 1_048_576.0 {
                format!("{:.2} MB", size_f / 1_048_576.0)
            } else if size_f >= 1_024.0 {
                format!("{:.2} KB", size_f / 1_024.0)
            } else {
                format!("{} B", size)
            }
        } else if self.is_calculating_size {
            "计算中...".to_string()
        } else {
            "-".to_string()
        }
    }
}
