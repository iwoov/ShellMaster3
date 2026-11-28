// Snippets 弹窗状态管理

use gpui_kit::{App, AppContext, Context, Entity, FocusHandle, Window};
use gpui_kit::component::input::{InputState, TextareaState};

use crate::components::common::dialog::keep_focus_in_dialog;
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

/// 需要校验的字段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetField {
    Name,
    Command,
}

/// 校验分组或命令：名称必填；命令弹窗里命令内容也必填。
/// 返回出错的字段和提示文字的 i18n key，按表单中的顺序排列。
pub fn validate_snippet(name: &str, command: Option<&str>) -> Vec<(SnippetField, &'static str)> {
    let mut errors = Vec::new();
    if name.trim().is_empty() {
        errors.push((SnippetField::Name, "snippets.dialog.error.name_required"));
    }
    if command.is_some_and(|command| command.trim().is_empty()) {
        errors.push((
            SnippetField::Command,
            "snippets.dialog.error.command_required",
        ));
    }
    errors
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
    pub command_input: Option<Entity<TextareaState>>,
    /// 当前父级 ID
    pub current_parent_id: Option<String>,
    /// 需要刷新页面
    pub needs_page_refresh: bool,
    /// 界面语言，由调用方在每次渲染前传入
    pub lang: Language,
    /// 点过保存且校验没通过之后为 true：此后实时显示校验结果
    show_errors: bool,
    /// 写入文件失败时的提示
    pub save_error: Option<String>,
    pub focus_handle: Option<FocusHandle>,
    /// 打开后第一次渲染时聚焦名称输入框
    focus_pending: bool,
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
            lang: Language::default(),
            show_errors: false,
            save_error: None,
            focus_handle: None,
            focus_pending: false,
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

    /// 确保输入框已创建（在 Window 上下文中调用），并处理打开时的焦点
    pub fn ensure_inputs_created(
        &mut self,
        lang: &Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.lang = lang.clone();
        let is_group = self.is_group_dialog();

        // 创建名称输入框
        if self.name_input.is_none() {
            let placeholder = i18n::t(lang, "snippets.dialog.enter_name");
            self.name_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }

        // 创建命令输入框（仅命令弹窗，多行，随内容增高）
        if !is_group && self.command_input.is_none() {
            let placeholder = i18n::t(lang, "snippets.dialog.enter_command");
            self.command_input = Some(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(3, 8)
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

        // 打开时聚焦名称输入框；之后保证焦点留在弹窗里，Esc 和回车始终有效
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            match &self.name_input {
                Some(input) => input.update(cx, |input, cx| input.focus(window, cx)),
                None => focus_handle.focus(window, cx),
            }
        }
        keep_focus_in_dialog(cx.weak_entity(), Self::is_open, &focus_handle, window);
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

    /// 重置输入框和提示（打开时调用）
    fn reset_inputs(&mut self) {
        self.name_input = None;
        self.command_input = None;
        self.pending_name_value = None;
        self.pending_command_value = None;
        self.show_errors = false;
        self.save_error = None;
        self.focus_pending = true;
    }

    /// 关闭弹窗
    pub fn close(&mut self) {
        self.dialog_mode = SnippetsDialogMode::Closed;
        self.editing_group = None;
        self.editing_command = None;
        self.reset_inputs();
        self.focus_pending = false;
    }

    /// 获取单行输入框的文本值
    fn get_input_text(input: &Option<Entity<InputState>>, cx: &App) -> String {
        input
            .as_ref()
            .map(|i| i.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// 获取多行输入框的文本值
    fn get_textarea_text(input: &Option<Entity<TextareaState>>, cx: &App) -> String {
        input
            .as_ref()
            .map(|i| i.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// 当前表单的校验结果
    fn errors(&self, cx: &App) -> Vec<(SnippetField, &'static str)> {
        let name = Self::get_input_text(&self.name_input, cx);
        let command =
            (!self.is_group_dialog()).then(|| Self::get_textarea_text(&self.command_input, cx));
        validate_snippet(&name, command.as_deref())
    }

    /// 当前要显示的校验错误（还没点过保存时不显示）
    pub fn visible_errors(&self, cx: &App) -> Vec<(SnippetField, &'static str)> {
        if self.show_errors {
            self.errors(cx)
        } else {
            Vec::new()
        }
    }

    /// 校验并保存。校验不通过时聚焦出错的输入框；写入失败时显示错误，弹窗不关闭。
    pub fn try_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let errors = self.errors(cx);
        if let Some(&(field, _)) = errors.first() {
            self.show_errors = true;
            self.save_error = None;
            match field {
                SnippetField::Name => {
                    if let Some(input) = &self.name_input {
                        input.update(cx, |input, cx| input.focus(window, cx));
                    }
                }
                SnippetField::Command => {
                    if let Some(input) = &self.command_input {
                        input.update(cx, |input, cx| input.focus(window, cx));
                    }
                }
            }
            cx.notify();
            return;
        }

        let name = Self::get_input_text(&self.name_input, cx)
            .trim()
            .to_string();
        let result = if self.is_group_dialog() {
            self.editing_group.clone().map(|mut group| {
                group.name = name;
                if self.dialog_mode == SnippetsDialogMode::AddGroup {
                    storage::add_snippet_group(group)
                } else {
                    storage::update_snippet_group(group)
                }
            })
        } else {
            let command = Self::get_textarea_text(&self.command_input, cx);
            self.editing_command.clone().map(|mut cmd| {
                cmd.name = name;
                cmd.command = command;
                if self.dialog_mode == SnippetsDialogMode::AddCommand {
                    storage::add_snippet_command(cmd)
                } else {
                    storage::update_snippet_command(cmd)
                }
            })
        };

        match result {
            Some(Err(err)) => {
                self.save_error = Some(format!(
                    "{}{:#}",
                    i18n::t(&self.lang, "snippets.dialog.save_failed"),
                    err
                ));
            }
            saved => {
                self.needs_page_refresh = saved.is_some();
                self.close();
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_snippet, SnippetField};

    fn fields(name: &str, command: Option<&str>) -> Vec<SnippetField> {
        validate_snippet(name, command)
            .into_iter()
            .map(|(field, _)| field)
            .collect()
    }

    #[test]
    fn names_and_commands_are_required() {
        // 分组只需要名称
        assert_eq!(fields("运维", None), vec![]);
        assert_eq!(fields("  ", None), vec![SnippetField::Name]);
        // 命令还需要命令内容
        assert_eq!(fields("磁盘", Some("df -h")), vec![]);
        assert_eq!(fields("磁盘", Some(" \n ")), vec![SnippetField::Command]);
        assert_eq!(
            fields("", Some("")),
            vec![SnippetField::Name, SnippetField::Command]
        );
    }
}
