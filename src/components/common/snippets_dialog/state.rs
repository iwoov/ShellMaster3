// Snippets 弹窗状态管理

use gpui::{App, AppContext, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::i18n;
use crate::models::settings::Language;
use crate::models::{SnippetCommand, SnippetGroup};
use crate::services::storage;

/// 弹窗模式
#[derive(Clone, PartialEq, Eq, Default)]
pub enum SnippetsDialogMode {
    #[default]
    Closed,
    AddGroup,
    EditGroup,
    AddCommand,
    EditCommand,
}

/// Snippets 弹窗状态
pub struct SnippetsDialogState {
    /// 弹窗模式
    pub dialog_mode: SnippetsDialogMode,
    /// 编辑中的组
    pub editing_group: Option<SnippetGroup>,
    /// 编辑中的命令
    pub editing_command: Option<SnippetCommand>,
    /// 输入框：名称
    pub name_input: Option<Entity<InputState>>,
    /// 输入框：命令内容
    pub command_input: Option<Entity<InputState>>,
    /// 当前父级 ID
    pub current_parent_id: Option<String>,
    /// 需要刷新页面
    pub needs_page_refresh: bool,
    /// 待设置的初始值（用于编辑模式）
    pending_name_value: Option<String>,
    pending_command_value: Option<String>,
}

impl Default for SnippetsDialogState {
    fn default() -> Self {
        Self {
            dialog_mode: SnippetsDialogMode::Closed,
            editing_group: None,
            editing_command: None,
            name_input: None,
            command_input: None,
            current_parent_id: None,
            needs_page_refresh: false,
            pending_name_value: None,
            pending_command_value: None,
        }
    }
}

impl SnippetsDialogState {
    /// 是否显示弹窗
    pub fn is_open(&self) -> bool {
        self.dialog_mode != SnippetsDialogMode::Closed
    }

    /// 是否为组弹窗
    pub fn is_group_dialog(&self) -> bool {
        matches!(
            self.dialog_mode,
            SnippetsDialogMode::AddGroup | SnippetsDialogMode::EditGroup
        )
    }

    /// 设置当前父级 ID
    pub fn set_parent_id(&mut self, parent_id: Option<String>) {
        self.current_parent_id = parent_id;
    }

    /// 确保输入框已创建（在 Window 上下文中调用）
    pub fn ensure_inputs_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        let is_group = self.is_group_dialog();
        let name_placeholder = if is_group {
            i18n::t(&lang, "snippets.dialog.enter_name")
        } else {
            i18n::t(&lang, "snippets.dialog.enter_name")
        };

        // 创建名称输入框
        if self.name_input.is_none() {
            self.name_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(name_placeholder)));
        }

        // 创建命令输入框（仅命令弹窗，多行模式）
        if !is_group && self.command_input.is_none() {
            let cmd_placeholder = i18n::t(&lang, "snippets.dialog.enter_command");
            self.command_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(cmd_placeholder)
                    .auto_grow(2, 6) // 2-6 行自动增长
            }));
        }

        // 应用待设置的值（编辑模式）
        if let Some(name) = self.pending_name_value.take() {
            if let Some(input) = &self.name_input {
                input.update(cx, |s, cx| s.set_value(name, window, cx));
            }
        }
        if let Some(cmd) = self.pending_command_value.take() {
            if let Some(input) = &self.command_input {
                input.update(cx, |s, cx| s.set_value(cmd, window, cx));
            }
        }
    }

    /// 打开添加组弹窗
    pub fn open_add_group(&mut self) {
        self.reset_inputs();
        self.dialog_mode = SnippetsDialogMode::AddGroup;
        self.editing_group = Some(SnippetGroup {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: self.current_parent_id.clone(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            ..Default::default()
        });
    }

    /// 打开编辑组弹窗
    pub fn open_edit_group(&mut self, group: &SnippetGroup) {
        self.reset_inputs();
        self.dialog_mode = SnippetsDialogMode::EditGroup;
        self.editing_group = Some(group.clone());
        self.pending_name_value = Some(group.name.clone());
    }

    /// 打开添加命令弹窗
    pub fn open_add_command(&mut self) {
        self.reset_inputs();
        self.dialog_mode = SnippetsDialogMode::AddCommand;
        self.editing_command = Some(SnippetCommand {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: self.current_parent_id.clone(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            ..Default::default()
        });
    }

    /// 打开编辑命令弹窗
    pub fn open_edit_command(&mut self, command: &SnippetCommand) {
        self.reset_inputs();
        self.dialog_mode = SnippetsDialogMode::EditCommand;
        self.editing_command = Some(command.clone());
        self.pending_name_value = Some(command.name.clone());
        self.pending_command_value = Some(command.command.clone());
    }

    /// 重置输入框状态
    fn reset_inputs(&mut self) {
        self.name_input = None;
        self.command_input = None;
        self.pending_name_value = None;
        self.pending_command_value = None;
    }

    /// 关闭弹窗
    pub fn close(&mut self) {
        self.dialog_mode = SnippetsDialogMode::Closed;
        self.editing_group = None;
        self.editing_command = None;
        self.reset_inputs();
    }

    /// 获取输入框的文本值
    fn get_input_text(input: &Option<Entity<InputState>>, cx: &App) -> String {
        input
            .as_ref()
            .map(|i| i.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// 保存组
    pub fn save_group(&mut self, cx: &App) -> bool {
        if let Some(mut group) = self.editing_group.take() {
            group.name = Self::get_input_text(&self.name_input, cx);
            let result = if self.dialog_mode == SnippetsDialogMode::AddGroup {
                storage::add_snippet_group(group)
            } else {
                storage::update_snippet_group(group)
            };
            self.needs_page_refresh = result.is_ok();
            self.close();
            return result.is_ok();
        }
        false
    }

    /// 保存命令
    pub fn save_command(&mut self, cx: &App) -> bool {
        if let Some(mut cmd) = self.editing_command.take() {
            cmd.name = Self::get_input_text(&self.name_input, cx);
            cmd.command = Self::get_input_text(&self.command_input, cx);
            let result = if self.dialog_mode == SnippetsDialogMode::AddCommand {
                storage::add_snippet_command(cmd)
            } else {
                storage::update_snippet_command(cmd)
            };
            self.needs_page_refresh = result.is_ok();
            self.close();
            return result.is_ok();
        }
        false
    }
}
