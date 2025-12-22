// 子模块声明
pub mod helpers;
pub mod panels;

use gpui::prelude::*;
use gpui::*;
use gpui_component::input::InputState;
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::AppSettings;
use crate::services::storage;

// 导入辅助函数
use helpers::create_float_number_input;
use helpers::create_int_number_input;

// 导入面板函数
use panels::{
    render_about_panel, render_connection_panel, render_keybindings_panel, render_monitor_panel,
    render_sftp_panel, render_sync_panel, render_system_panel, render_terminal_panel,
    render_theme_panel,
};

/// 设置导航区域类型
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum SettingsSection {
    #[default]
    Theme,
    Terminal,
    KeyBindings,
    Sftp,
    Monitor,
    Connection,
    Sync,
    System,
    About,
}

impl SettingsSection {
    pub fn label_key(&self) -> &'static str {
        match self {
            SettingsSection::Theme => "settings.nav.theme",
            SettingsSection::Terminal => "settings.nav.terminal",
            SettingsSection::KeyBindings => "settings.nav.keybindings",
            SettingsSection::Sftp => "settings.nav.sftp",
            SettingsSection::Monitor => "settings.nav.monitor",
            SettingsSection::Connection => "settings.nav.connection",
            SettingsSection::Sync => "settings.nav.sync",
            SettingsSection::System => "settings.nav.system",
            SettingsSection::About => "settings.nav.about",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            SettingsSection::Theme => icons::GRID,
            SettingsSection::Terminal => icons::TERMINAL,
            SettingsSection::KeyBindings => icons::CODE,
            SettingsSection::Sftp => icons::FOLDER,
            SettingsSection::Monitor => icons::SERVER,
            SettingsSection::Connection => icons::LINK,
            SettingsSection::Sync => icons::CLOUD,
            SettingsSection::System => icons::SETTINGS,
            SettingsSection::About => icons::USER,
        }
    }
}

/// 设置弹窗状态
pub struct SettingsDialogState {
    pub visible: bool,
    pub current_section: SettingsSection,
    pub settings: AppSettings,
    /// 标记设置是否有变更
    pub has_changes: bool,

    // ============ 主题设置输入 ============
    pub ui_font_family_input: Option<Entity<InputState>>,
    pub ui_font_size_input: Option<Entity<InputState>>,

    // ============ 终端设置输入 ============
    pub terminal_font_family_input: Option<Entity<InputState>>,
    pub terminal_font_size_input: Option<Entity<InputState>>,
    pub terminal_line_height_input: Option<Entity<InputState>>,
    pub scrollback_lines_input: Option<Entity<InputState>>,

    // ============ 连接设置输入 ============
    pub default_port_input: Option<Entity<InputState>>,
    pub connection_timeout_input: Option<Entity<InputState>>,
    pub keepalive_interval_input: Option<Entity<InputState>>,
    pub reconnect_attempts_input: Option<Entity<InputState>>,
    pub reconnect_interval_input: Option<Entity<InputState>>,

    // ============ 监控设置输入 ============
    pub history_retention_input: Option<Entity<InputState>>,
    pub cpu_threshold_input: Option<Entity<InputState>>,
    pub memory_threshold_input: Option<Entity<InputState>>,
    pub disk_threshold_input: Option<Entity<InputState>>,

    // ============ SFTP 设置输入 ============
    pub concurrent_transfers_input: Option<Entity<InputState>>,
    pub local_default_path_input: Option<Entity<InputState>>,

    // ============ 同步设置输入 ============
    pub webdav_url_input: Option<Entity<InputState>>,
    pub webdav_username_input: Option<Entity<InputState>>,
    pub webdav_password_input: Option<Entity<InputState>>,
    pub webdav_path_input: Option<Entity<InputState>>,

    // ============ 系统设置输入 ============
    pub log_retention_input: Option<Entity<InputState>>,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        let settings = storage::load_settings().unwrap_or_default();
        Self {
            visible: false,
            current_section: SettingsSection::Theme,
            settings,
            has_changes: false,
            // 主题
            ui_font_family_input: None,
            ui_font_size_input: None,
            // 终端
            terminal_font_family_input: None,
            terminal_font_size_input: None,
            terminal_line_height_input: None,
            scrollback_lines_input: None,
            // 连接
            default_port_input: None,
            connection_timeout_input: None,
            keepalive_interval_input: None,
            reconnect_attempts_input: None,
            reconnect_interval_input: None,
            // 监控
            history_retention_input: None,
            cpu_threshold_input: None,
            memory_threshold_input: None,
            disk_threshold_input: None,
            // SFTP
            concurrent_transfers_input: None,
            local_default_path_input: None,
            // 同步
            webdav_url_input: None,
            webdav_username_input: None,
            webdav_password_input: None,
            webdav_path_input: None,
            // 系统
            log_retention_input: None,
        }
    }
}

impl SettingsDialogState {
    pub fn open(&mut self) {
        // 打开时重新加载设置
        self.settings = storage::load_settings().unwrap_or_default();
        self.visible = true;
        self.current_section = SettingsSection::Theme;
        self.has_changes = false;
        // 清除输入状态以便重新加载
        self.reset_inputs();
    }

    /// 重置所有输入框状态
    fn reset_inputs(&mut self) {
        self.ui_font_family_input = None;
        self.ui_font_size_input = None;
        self.terminal_font_family_input = None;
        self.terminal_font_size_input = None;
        self.terminal_line_height_input = None;
        self.scrollback_lines_input = None;
        self.default_port_input = None;
        self.connection_timeout_input = None;
        self.keepalive_interval_input = None;
        self.reconnect_attempts_input = None;
        self.reconnect_interval_input = None;
        self.history_retention_input = None;
        self.cpu_threshold_input = None;
        self.memory_threshold_input = None;
        self.disk_threshold_input = None;
        self.concurrent_transfers_input = None;
        self.local_default_path_input = None;
        self.webdav_url_input = None;
        self.webdav_username_input = None;
        self.webdav_password_input = None;
        self.webdav_path_input = None;
        self.log_retention_input = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn save(&mut self) {
        if let Err(e) = storage::save_settings(&self.settings) {
            eprintln!("保存设置失败: {}", e);
        }
        self.has_changes = false;
    }

    /// 标记设置已变更
    pub fn mark_changed(&mut self) {
        self.has_changes = true;
    }

    /// 确保输入框已创建（在有 window 上下文时调用）
    pub fn ensure_inputs_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 主题设置
        if self.ui_font_family_input.is_none() {
            let value = self.settings.theme.ui_font_family.clone();
            self.ui_font_family_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.ui_font_size_input.is_none() {
            let value = self.settings.theme.ui_font_size.to_string();
            self.ui_font_size_input = Some(create_int_number_input(value, 8, 144, 1, window, cx));
        }

        // 终端设置
        if self.terminal_font_family_input.is_none() {
            let value = self.settings.terminal.font_family.clone();
            self.terminal_font_family_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.terminal_font_size_input.is_none() {
            let value = self.settings.terminal.font_size.to_string();
            self.terminal_font_size_input =
                Some(create_int_number_input(value, 8, 144, 1, window, cx));
        }
        if self.terminal_line_height_input.is_none() {
            let value = format!("{:.1}", self.settings.terminal.line_height);
            self.terminal_line_height_input =
                Some(create_float_number_input(value, 0.8, 3.0, 0.1, window, cx));
        }
        if self.scrollback_lines_input.is_none() {
            let value = self.settings.terminal.scrollback_lines.to_string();
            self.scrollback_lines_input =
                Some(create_int_number_input(value, 100, 100000, 100, window, cx));
        }

        // 连接设置
        if self.default_port_input.is_none() {
            let value = self.settings.connection.default_port.to_string();
            self.default_port_input = Some(create_int_number_input(value, 1, 65535, 1, window, cx));
        }
        if self.connection_timeout_input.is_none() {
            let value = self.settings.connection.connection_timeout_secs.to_string();
            self.connection_timeout_input =
                Some(create_int_number_input(value, 1, 300, 1, window, cx));
        }
        if self.keepalive_interval_input.is_none() {
            let value = self.settings.connection.keepalive_interval_secs.to_string();
            self.keepalive_interval_input =
                Some(create_int_number_input(value, 0, 3600, 1, window, cx));
        }
        if self.reconnect_attempts_input.is_none() {
            let value = self.settings.connection.reconnect_attempts.to_string();
            self.reconnect_attempts_input =
                Some(create_int_number_input(value, 1, 100, 1, window, cx));
        }
        if self.reconnect_interval_input.is_none() {
            let value = self.settings.connection.reconnect_interval_secs.to_string();
            self.reconnect_interval_input =
                Some(create_int_number_input(value, 1, 300, 1, window, cx));
        }

        // 监控设置
        if self.history_retention_input.is_none() {
            let value = self.settings.monitor.history_retention_minutes.to_string();
            self.history_retention_input =
                Some(create_int_number_input(value, 1, 1440, 1, window, cx));
        }
        if self.cpu_threshold_input.is_none() {
            let value = self.settings.monitor.cpu_alert_threshold.to_string();
            self.cpu_threshold_input = Some(create_int_number_input(value, 0, 100, 1, window, cx));
        }
        if self.memory_threshold_input.is_none() {
            let value = self.settings.monitor.memory_alert_threshold.to_string();
            self.memory_threshold_input =
                Some(create_int_number_input(value, 0, 100, 1, window, cx));
        }
        if self.disk_threshold_input.is_none() {
            let value = self.settings.monitor.disk_alert_threshold.to_string();
            self.disk_threshold_input = Some(create_int_number_input(value, 0, 100, 1, window, cx));
        }

        // SFTP 设置
        let lang = &self.settings.theme.language;
        if self.concurrent_transfers_input.is_none() {
            let value = self.settings.sftp.concurrent_transfers.to_string();
            self.concurrent_transfers_input =
                Some(create_int_number_input(value, 1, 10, 1, window, cx));
        }
        if self.local_default_path_input.is_none() {
            let value = self.settings.sftp.local_default_path.clone();
            let placeholder = i18n::t(lang, "settings.sftp.default_download_path_placeholder");
            self.local_default_path_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(value, window, cx);
                state
            }));
        }

        // 同步设置
        if self.webdav_url_input.is_none() {
            let value = self.settings.sync.webdav_url.clone();
            self.webdav_url_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder("https://...");
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.webdav_username_input.is_none() {
            let value = self.settings.sync.webdav_username.clone();
            let placeholder = i18n::t(lang, "settings.sync.webdav_username");
            self.webdav_username_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.webdav_password_input.is_none() {
            let value = self.settings.sync.webdav_password.clone();
            let placeholder = i18n::t(lang, "settings.sync.webdav_password");
            self.webdav_password_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(true);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.webdav_path_input.is_none() {
            let value = self.settings.sync.webdav_path.clone();
            self.webdav_path_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder("/shellmaster");
                state.set_value(value, window, cx);
                state
            }));
        }

        // 系统设置
        if self.log_retention_input.is_none() {
            let value = self.settings.system.log_retention_days.to_string();
            self.log_retention_input = Some(create_int_number_input(value, 1, 365, 1, window, cx));
        }
    }

    /// 从 InputState 同步值到 settings
    pub fn sync_from_inputs(&mut self, cx: &App) {
        // 主题
        if let Some(input) = &self.ui_font_family_input {
            self.settings.theme.ui_font_family = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.ui_font_size_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.theme.ui_font_size = v;
            }
        }

        // 终端
        if let Some(input) = &self.terminal_font_family_input {
            self.settings.terminal.font_family = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.terminal_font_size_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.terminal.font_size = v;
            }
        }
        if let Some(input) = &self.terminal_line_height_input {
            if let Ok(v) = input.read(cx).value().parse::<f32>() {
                self.settings.terminal.line_height = v;
            }
        }
        if let Some(input) = &self.scrollback_lines_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.terminal.scrollback_lines = v;
            }
        }

        // 连接
        if let Some(input) = &self.default_port_input {
            if let Ok(v) = input.read(cx).value().parse::<u16>() {
                self.settings.connection.default_port = v;
            }
        }
        if let Some(input) = &self.connection_timeout_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.connection.connection_timeout_secs = v;
            }
        }
        if let Some(input) = &self.keepalive_interval_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.connection.keepalive_interval_secs = v;
            }
        }
        if let Some(input) = &self.reconnect_attempts_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.connection.reconnect_attempts = v;
            }
        }
        if let Some(input) = &self.reconnect_interval_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.connection.reconnect_interval_secs = v;
            }
        }

        // 监控
        if let Some(input) = &self.history_retention_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.monitor.history_retention_minutes = v;
            }
        }
        if let Some(input) = &self.cpu_threshold_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.monitor.cpu_alert_threshold = v;
            }
        }
        if let Some(input) = &self.memory_threshold_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.monitor.memory_alert_threshold = v;
            }
        }
        if let Some(input) = &self.disk_threshold_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.monitor.disk_alert_threshold = v;
            }
        }

        // SFTP
        if let Some(input) = &self.local_default_path_input {
            self.settings.sftp.local_default_path = input.read(cx).value().to_string();
        }

        // 同步
        if let Some(input) = &self.webdav_url_input {
            self.settings.sync.webdav_url = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_username_input {
            self.settings.sync.webdav_username = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_password_input {
            self.settings.sync.webdav_password = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_path_input {
            self.settings.sync.webdav_path = input.read(cx).value().to_string();
        }

        // 系统
        if let Some(input) = &self.log_retention_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.system.log_retention_days = v;
            }
        }
    }
}

/// 渲染设置弹窗覆盖层
pub fn render_settings_dialog_overlay(
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let state_for_content = state.clone();

    div()
        .id("settings-dialog-container")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        // 背景遮罩层
        .child(
            div()
                .id("settings-dialog-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000080))
                .on_click(move |_, _, cx| {
                    state_for_close.update(cx, |s, _| s.close());
                }),
        )
        // 弹窗内容
        .child(render_dialog_content(state_for_content, cx))
}

/// 渲染弹窗内容
fn render_dialog_content(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_for_nav = state.clone();
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    // 使用全局主题帮助函数
    let bg_color = crate::theme::popover_color(cx);
    let border_color = cx.theme().border;

    div()
        .id("settings-dialog-content")
        .w(px(800.))
        .h(px(560.))
        .bg(bg_color)
        .border_1()
        .border_color(border_color)
        .rounded_lg()
        .shadow_lg()
        .flex()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        // 阻止滚动事件穿透到底层内容
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_left_nav(state_for_nav, cx))
        .child(render_right_content(
            state,
            state_for_cancel,
            state_for_save,
            cx,
        ))
}

/// 渲染左侧导航菜单
fn render_left_nav(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let sections = [
        SettingsSection::Theme,
        SettingsSection::Terminal,
        SettingsSection::KeyBindings,
        SettingsSection::Sftp,
        SettingsSection::Monitor,
        SettingsSection::Connection,
        SettingsSection::Sync,
        SettingsSection::System,
        SettingsSection::About,
    ];

    let bg_color = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;

    div()
        .w(px(180.))
        .h_full()
        .bg(bg_color)
        .rounded_l_lg()
        .border_r_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .p_4()
        .gap_1()
        .children(sections.into_iter().map(|section| {
            let state = state.clone();
            render_nav_item(state, section, cx)
        }))
}

/// 渲染导航项
fn render_nav_item(
    state: Entity<SettingsDialogState>,
    section: SettingsSection,
    cx: &App,
) -> impl IntoElement {
    let state_for_click = state.clone();
    let hover_bg = cx.theme().muted;
    let icon_color = cx.theme().muted_foreground;
    let text_color = cx.theme().foreground;
    let lang = &state.read(cx).settings.theme.language;

    div()
        .id(SharedString::from(format!("settings-nav-{:?}", section)))
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            state_for_click.update(cx, |s, _| {
                s.current_section = section;
            });
        })
        .child(render_icon(section.icon(), icon_color.into()))
        .child(
            div()
                .text_sm()
                .text_color(text_color)
                .child(i18n::t(lang, section.label_key())),
        )
}

/// 渲染右侧内容区域
fn render_right_content(
    state: Entity<SettingsDialogState>,
    state_for_cancel: Entity<SettingsDialogState>,
    state_for_save: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_panel = state.clone();
    let border_color = cx.theme().border;
    let title_color = cx.theme().foreground;

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        // 标题栏
        .child(
            div()
                .h(px(56.))
                .flex_shrink_0()
                .border_b_1()
                .border_color(border_color)
                .flex()
                .items_center()
                .px_6()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(title_color)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(title_color)
                        .child(i18n::t(
                            &state.read(cx).settings.theme.language,
                            "settings.title",
                        )),
                ),
        )
        // 内容区域
        .child(
            div()
                .id("settings-form-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scrollbar()
                .p_6()
                .child(render_section_content(state_for_panel, cx)),
        )
        // 底部按钮
        .child(render_footer_buttons(state_for_cancel, state_for_save, cx))
}

/// 渲染当前分区内容
fn render_section_content(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let section = state.read(cx).current_section;

    match section {
        SettingsSection::Theme => render_theme_panel(state, cx).into_any_element(),
        SettingsSection::Terminal => render_terminal_panel(state, cx).into_any_element(),
        SettingsSection::KeyBindings => render_keybindings_panel(state, cx).into_any_element(),
        SettingsSection::Sftp => render_sftp_panel(state, cx).into_any_element(),
        SettingsSection::Monitor => render_monitor_panel(state, cx).into_any_element(),
        SettingsSection::Connection => render_connection_panel(state, cx).into_any_element(),
        SettingsSection::Sync => render_sync_panel(state, cx).into_any_element(),
        SettingsSection::System => render_system_panel(state, cx).into_any_element(),
        SettingsSection::About => render_about_panel(state, cx).into_any_element(),
    }
}

/// 渲染底部按钮
fn render_footer_buttons(
    state_for_cancel: Entity<SettingsDialogState>,
    state_for_save: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let border_color = cx.theme().border;
    let secondary_bg = cx.theme().secondary;
    let secondary_hover = cx.theme().secondary_hover;
    let text_color = cx.theme().foreground;
    let primary_bg = cx.theme().primary;
    let primary_hover = cx.theme().primary_hover;
    let primary_fg = cx.theme().primary_foreground;
    let lang = &state_for_cancel.read(cx).settings.theme.language;

    div()
        .h(px(64.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(border_color)
        .flex()
        .items_center()
        .justify_end()
        .gap_3()
        .px_6()
        // 取消按钮
        .child(
            div()
                .id("settings-cancel-btn")
                .px_4()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(border_color)
                .bg(secondary_bg)
                .cursor_pointer()
                .hover(move |s| s.bg(secondary_hover))
                .on_click(move |_, _, cx| {
                    state_for_cancel.update(cx, |s, _| s.close());
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(text_color)
                        .child(i18n::t(lang, "common.cancel")),
                ),
        )
        // 保存按钮
        .child(
            div()
                .id("settings-save-btn")
                .px_4()
                .py_2()
                .rounded_md()
                .bg(primary_bg)
                .cursor_pointer()
                .hover(move |s| s.bg(primary_hover))
                .on_click(move |_, _, cx| {
                    state_for_save.update(cx, |s, cx| {
                        s.sync_from_inputs(cx);
                        s.save();
                        s.close();
                    });
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(primary_fg)
                        .child(i18n::t(lang, "common.save")),
                ),
        )
}
