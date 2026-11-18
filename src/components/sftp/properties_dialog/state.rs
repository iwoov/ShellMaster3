// 属性对话框状态管理

use gpui::{Context, FocusHandle, Window};
use tokio_util::sync::CancellationToken;

use crate::components::common::dialog::keep_focus_in_dialog;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::FileEntry;

/// 属性对话框状态
#[derive(Default)]
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
    /// 界面语言，由调用方在每次渲染前传入
    pub lang: Language,
    pub focus_handle: Option<FocusHandle>,
    /// 打开后第一次渲染时聚焦弹窗（Esc 才能关闭）
    focus_pending: bool,
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
        self.focus_pending = true;
    }

    /// 渲染前调用：记下界面语言，打开后第一次渲染时聚焦弹窗，之后保证焦点留在弹窗里
    pub fn prepare(&mut self, lang: &Language, window: &mut Window, cx: &mut Context<Self>) {
        self.lang = lang.clone();
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            focus_handle.focus(window);
        }
        keep_focus_in_dialog(
            cx.weak_entity(),
            |s: &Self| s.is_open,
            &focus_handle,
            window,
        );
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
        self.focus_pending = false;
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
    pub fn format_folder_size(&self, lang: &Language) -> String {
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
            i18n::t(lang, "sftp.properties.calculating").to_string()
        } else {
            "-".to_string()
        }
    }
}
