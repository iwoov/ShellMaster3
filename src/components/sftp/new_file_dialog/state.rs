// 新建文件对话框状态管理

use gpui::{App, AppContext, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

/// 新建文件对话框状态
pub struct NewFileDialogState {
    /// 是否打开
    pub is_open: bool,
    /// 当前路径（父目录）
    pub current_path: String,
    /// 文件名称输入框
    pub name_input: Option<Entity<InputState>>,
    /// 错误信息
    pub error_message: Option<String>,
    /// 是否正在创建中
    pub is_creating: bool,
    /// 关联的 tab_id
    pub tab_id: String,
}

impl Default for NewFileDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            current_path: String::new(),
            name_input: None,
            error_message: None,
            is_creating: false,
            tab_id: String::new(),
        }
    }
}

impl NewFileDialogState {
    /// 打开对话框
    pub fn open(&mut self, current_path: String, tab_id: String) {
        self.is_open = true;
        self.current_path = current_path;
        self.tab_id = tab_id;
        self.error_message = None;
        self.is_creating = false;
        // 重置输入框（将在渲染时创建）
        self.name_input = None;
    }

    /// 关闭对话框
    pub fn close(&mut self) {
        self.is_open = false;
        self.current_path.clear();
        self.tab_id.clear();
        self.name_input = None;
        self.error_message = None;
        self.is_creating = false;
    }

    /// 确保输入框已创建
    pub fn ensure_input_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.name_input.is_none() {
            let lang = storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or(Language::Chinese);
            let placeholder = i18n::t(&lang, "sftp.new_file.placeholder");
            self.name_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
    }

    /// 获取输入的文件名称
    pub fn get_file_name(&self, cx: &App) -> String {
        self.name_input
            .as_ref()
            .map(|i| i.read(cx).text().to_string().trim().to_string())
            .unwrap_or_default()
    }

    /// 验证文件名称
    pub fn validate_name(&mut self, cx: &App) -> bool {
        let name = self.get_file_name(cx);
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        if name.is_empty() {
            self.error_message = Some(i18n::t(&lang, "sftp.new_file.error_empty").to_string());
            return false;
        }

        // 检查非法字符
        if name.contains('/') || name.contains('\0') {
            self.error_message = Some(i18n::t(&lang, "sftp.new_file.error_invalid").to_string());
            return false;
        }

        self.error_message = None;
        true
    }

    /// 获取完整的新文件路径
    pub fn get_full_path(&self, cx: &App) -> String {
        let name = self.get_file_name(cx);
        if self.current_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", self.current_path, name)
        }
    }

    /// 设置错误信息
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.is_creating = false;
    }

    /// 开始创建
    pub fn start_creating(&mut self) {
        self.is_creating = true;
        self.error_message = None;
    }
}
