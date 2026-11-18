// 新建文件夹对话框状态管理

use gpui::{App, AppContext, Context, Entity, FocusHandle, Window};
use gpui_component::input::InputState;

use crate::components::common::dialog::keep_focus_in_dialog;
use crate::i18n;
use crate::models::settings::Language;

/// 新建文件夹对话框状态
#[derive(Default)]
pub struct NewFolderDialogState {
    /// 是否打开
    pub is_open: bool,
    /// 当前路径（父目录）
    pub current_path: String,
    /// 文件夹名称输入框
    pub name_input: Option<Entity<InputState>>,
    /// 错误信息
    pub error_message: Option<String>,
    /// 是否正在创建中
    pub is_creating: bool,
    /// 关联的 tab_id
    pub tab_id: String,
    /// 界面语言，由调用方在每次渲染前传入
    pub lang: Language,
    pub focus_handle: Option<FocusHandle>,
    /// 打开后第一次渲染时聚焦输入框
    focus_pending: bool,
}

impl NewFolderDialogState {
    /// 打开对话框
    pub fn open(&mut self, current_path: String, tab_id: String) {
        self.is_open = true;
        self.current_path = current_path;
        self.tab_id = tab_id;
        self.error_message = None;
        self.is_creating = false;
        // 重置输入框（将在渲染时创建）
        self.name_input = None;
        self.focus_pending = true;
    }

    /// 关闭对话框
    pub fn close(&mut self) {
        self.is_open = false;
        self.current_path.clear();
        self.tab_id.clear();
        self.name_input = None;
        self.error_message = None;
        self.is_creating = false;
        self.focus_pending = false;
    }

    /// 确保输入框已创建，并处理打开时的焦点
    pub fn ensure_input_created(
        &mut self,
        lang: &Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.lang = lang.clone();
        if self.name_input.is_none() {
            let placeholder = i18n::t(lang, "sftp.new_folder.placeholder");
            self.name_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }

        // 打开时聚焦输入框；之后保证焦点留在弹窗里，Esc 和回车始终有效
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            match &self.name_input {
                Some(input) => input.update(cx, |input, cx| input.focus(window, cx)),
                None => focus_handle.focus(window),
            }
        }
        keep_focus_in_dialog(
            cx.weak_entity(),
            |s: &Self| s.is_open,
            &focus_handle,
            window,
        );
    }

    /// 获取输入的文件夹名称
    pub fn get_folder_name(&self, cx: &App) -> String {
        self.name_input
            .as_ref()
            .map(|i| i.read(cx).text().to_string().trim().to_string())
            .unwrap_or_default()
    }

    /// 验证文件夹名称
    pub fn validate_name(&mut self, cx: &App) -> bool {
        let name = self.get_folder_name(cx);

        if name.is_empty() {
            self.error_message =
                Some(i18n::t(&self.lang, "sftp.new_folder.error_empty").to_string());
            return false;
        }

        // 检查非法字符
        if name.contains('/') || name.contains('\0') {
            self.error_message =
                Some(i18n::t(&self.lang, "sftp.new_folder.error_invalid").to_string());
            return false;
        }

        self.error_message = None;
        true
    }

    /// 获取完整的新文件夹路径
    pub fn get_full_path(&self, cx: &App) -> String {
        let name = self.get_folder_name(cx);
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
