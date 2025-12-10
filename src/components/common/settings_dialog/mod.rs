use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState, NumberInput};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::theme::{Theme as GpuiTheme, ThemeMode as GpuiThemeMode};
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::models::settings::{AppSettings, Language, ThemeMode};
use crate::services::storage;

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
    pub fn label(&self) -> &'static str {
        match self {
            SettingsSection::Theme => "主题设置",
            SettingsSection::Terminal => "终端设置",
            SettingsSection::KeyBindings => "按键绑定",
            SettingsSection::Sftp => "SFTP 设置",
            SettingsSection::Monitor => "监控设置",
            SettingsSection::Connection => "连接设置",
            SettingsSection::Sync => "数据同步",
            SettingsSection::System => "系统配置",
            SettingsSection::About => "关于",
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

    // ============ 同步设置输入 ============
    pub webdav_url_input: Option<Entity<InputState>>,
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
            // 同步
            webdav_url_input: None,
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
        self.webdav_url_input = None;
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
            self.ui_font_size_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
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
            self.terminal_font_size_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.terminal_line_height_input.is_none() {
            let value = format!("{:.1}", self.settings.terminal.line_height);
            self.terminal_line_height_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.scrollback_lines_input.is_none() {
            let value = self.settings.terminal.scrollback_lines.to_string();
            self.scrollback_lines_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }

        // 连接设置
        if self.default_port_input.is_none() {
            let value = self.settings.connection.default_port.to_string();
            self.default_port_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.connection_timeout_input.is_none() {
            let value = self.settings.connection.connection_timeout_secs.to_string();
            self.connection_timeout_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.keepalive_interval_input.is_none() {
            let value = self.settings.connection.keepalive_interval_secs.to_string();
            self.keepalive_interval_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.reconnect_attempts_input.is_none() {
            let value = self.settings.connection.reconnect_attempts.to_string();
            self.reconnect_attempts_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.reconnect_interval_input.is_none() {
            let value = self.settings.connection.reconnect_interval_secs.to_string();
            self.reconnect_interval_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }

        // 监控设置
        if self.history_retention_input.is_none() {
            let value = self.settings.monitor.history_retention_minutes.to_string();
            self.history_retention_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.cpu_threshold_input.is_none() {
            let value = self.settings.monitor.cpu_alert_threshold.to_string();
            self.cpu_threshold_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.memory_threshold_input.is_none() {
            let value = self.settings.monitor.memory_alert_threshold.to_string();
            self.memory_threshold_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.disk_threshold_input.is_none() {
            let value = self.settings.monitor.disk_alert_threshold.to_string();
            self.disk_threshold_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }

        // SFTP 设置
        if self.concurrent_transfers_input.is_none() {
            let value = self.settings.sftp.concurrent_transfers.to_string();
            self.concurrent_transfers_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
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
            self.log_retention_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
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

        // 同步
        if let Some(input) = &self.webdav_url_input {
            self.settings.sync.webdav_url = input.read(cx).value().to_string();
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
                .child(section.label()),
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
                        .child("设置"),
                ),
        )
        // 内容区域
        .child(
            div()
                .id("settings-form-scroll")
                .flex_1()
                .overflow_scroll()
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
        SettingsSection::KeyBindings => render_keybindings_panel(cx).into_any_element(),
        SettingsSection::Sftp => render_sftp_panel(state, cx).into_any_element(),
        SettingsSection::Monitor => render_monitor_panel(state, cx).into_any_element(),
        SettingsSection::Connection => render_connection_panel(state, cx).into_any_element(),
        SettingsSection::Sync => render_sync_panel(state, cx).into_any_element(),
        SettingsSection::System => render_system_panel(state, cx).into_any_element(),
        SettingsSection::About => render_about_panel(cx).into_any_element(),
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
                .child(div().text_sm().text_color(text_color).child("取消")),
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
                .child(div().text_sm().text_color(primary_fg).child("保存")),
        )
}

// ======================== 面板实现 ========================

/// 渲染主题设置面板
fn render_theme_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let current_mode = state_read.settings.theme.mode.clone();
    let current_language = state_read.settings.theme.language.clone();

    // 获取输入状态
    let ui_font_family_input = state_read.ui_font_family_input.clone();
    let ui_font_size_input = state_read.ui_font_size_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 语言设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("语言 / Language", cx))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_language_button(
                            state.clone(),
                            Language::Chinese,
                            current_language == Language::Chinese,
                            cx,
                        ))
                        .child(render_language_button(
                            state.clone(),
                            Language::English,
                            current_language == Language::English,
                            cx,
                        )),
                ),
        )
        // 外观模式
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("外观模式", cx))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Light,
                            "浅色模式",
                            current_mode == ThemeMode::Light,
                            cx,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Dark,
                            "深色模式",
                            current_mode == ThemeMode::Dark,
                            cx,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::System,
                            "跟随系统",
                            current_mode == ThemeMode::System,
                            cx,
                        )),
                ),
        )
        // 字体设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("字体设置", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(
                            ui_font_family_input.as_ref().map(|input| {
                                render_font_input_row(cx, "界面字体", input, UI_FONTS)
                            }),
                        )
                        .children(
                            ui_font_size_input
                                .as_ref()
                                .map(|input| render_number_row("界面字号", input, cx)),
                        ),
                ),
        )
}

/// 渲染语言按钮
fn render_language_button(
    state: Entity<SettingsDialogState>,
    lang: Language,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let bg_color = if selected {
        cx.theme().primary
    } else {
        cx.theme().secondary
    };
    let text_color = if selected {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };
    let label = lang.label();

    let hover_selected_bg = cx.theme().primary_hover;
    let hover_unselected_bg = cx.theme().secondary_hover;

    div()
        .id(SharedString::from(format!("lang-{:?}", lang)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(move |s| {
            if selected {
                s.bg(hover_selected_bg)
            } else {
                s.bg(hover_unselected_bg)
            }
        })
        .on_click(move |_, _, cx| {
            state.update(cx, |s, _| {
                s.settings.theme.language = lang.clone();
                s.mark_changed();
            });
        })
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染主题模式按钮
fn render_theme_mode_button(
    state: Entity<SettingsDialogState>,
    mode: ThemeMode,
    label: &'static str,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let bg_color = if selected {
        cx.theme().primary
    } else {
        cx.theme().secondary
    };
    let text_color = if selected {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };

    let hover_selected_bg = cx.theme().primary_hover;
    let hover_unselected_bg = cx.theme().secondary_hover;

    div()
        .id(SharedString::from(format!("theme-mode-{:?}", mode)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(move |s| {
            if selected {
                s.bg(hover_selected_bg)
            } else {
                s.bg(hover_unselected_bg)
            }
        })
        .on_click(move |_, window, cx| {
            // Save to settings
            state.update(cx, |s, _| {
                s.settings.theme.mode = mode.clone();
                s.mark_changed();
            });

            // Apply theme immediately
            match mode {
                ThemeMode::Light => GpuiTheme::change(GpuiThemeMode::Light, Some(window), cx),
                ThemeMode::Dark => GpuiTheme::change(GpuiThemeMode::Dark, Some(window), cx),
                ThemeMode::System => GpuiTheme::sync_system_appearance(Some(window), cx),
            }
        })
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染终端设置面板
fn render_terminal_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let terminal = &state_read.settings.terminal;

    // 获取输入状态
    let font_family_input = state_read.terminal_font_family_input.clone();
    let font_size_input = state_read.terminal_font_size_input.clone();
    let line_height_input = state_read.terminal_line_height_input.clone();
    let scrollback_input = state_read.scrollback_lines_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 字体
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("字体", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(font_family_input.as_ref().map(|input| {
                            render_font_input_row(cx, "终端字体", input, TERMINAL_FONTS)
                        }))
                        .children(
                            font_size_input
                                .as_ref()
                                .map(|input| render_number_row("字号", input, cx)),
                        )
                        .children(
                            line_height_input
                                .as_ref()
                                .map(|input| render_number_row("行高", input, cx)),
                        )
                        .child(render_switch_row(
                            "terminal-ligatures",
                            "启用连字",
                            terminal.ligatures,
                            state.clone(),
                            |s, v| s.settings.terminal.ligatures = v,
                            cx,
                        )),
                ),
        )
        // 配色
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("配色方案", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_theme_select_row(
                            "终端主题",
                            &terminal.color_scheme,
                            TERMINAL_THEMES,
                            state.clone(),
                            cx,
                        )),
                ),
        )
        // 显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("显示", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_switch_row(
                            "terminal-cursor-blink",
                            "光标闪烁",
                            terminal.cursor_blink,
                            state.clone(),
                            |s, v| s.settings.terminal.cursor_blink = v,
                            cx,
                        ))
                        .children(
                            scrollback_input
                                .as_ref()
                                .map(|input| render_number_row("滚动缓冲区", input, cx)),
                        ),
                ),
        )
}

/// 渲染关于面板
fn render_about_panel(cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
        .pt_8()
        // Logo / 应用名
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(64.))
                        .h(px(64.))
                        .rounded_xl()
                        .bg(cx.theme().primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(cx.theme().primary_foreground)
                                .child("SM"),
                        ),
                )
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child("ShellMaster"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("v1.0.0"),
                ),
        )
        // 技术信息
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_about_row("平台", "macOS", cx))
                .child(render_about_row("架构", std::env::consts::ARCH, cx))
                .child(render_about_row("Rust", env!("CARGO_PKG_RUST_VERSION"), cx)),
        )
        // 版权
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("© 2024 ShellMaster. All rights reserved."),
        )
}

/// 渲染按键绑定面板
fn render_keybindings_panel(cx: &App) -> impl IntoElement {
    div().flex().flex_col().gap_6().child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .h(px(200.))
            .child(
                div()
                    .text_base()
                    .text_color(cx.theme().muted_foreground)
                    .child("按键绑定编辑器将在后续版本实现"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .mt_2()
                    .child("可自定义终端和SFTP快捷键"),
            ),
    )
}

/// 渲染SFTP设置面板
fn render_sftp_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let sftp = &state.read(cx).settings.sftp;

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 文件显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("文件显示", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sftp-show-hidden",
                            "显示隐藏文件",
                            sftp.show_hidden_files,
                            state.clone(),
                            |s, v| s.settings.sftp.show_hidden_files = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-folders-first",
                            "文件夹优先",
                            sftp.folders_first,
                            state.clone(),
                            |s, v| s.settings.sftp.folders_first = v,
                            cx,
                        )),
                ),
        )
        // 传输设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("传输设置", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(
                            state
                                .read(cx)
                                .concurrent_transfers_input
                                .as_ref()
                                .map(|input| render_number_row("并发传输数", input, cx)),
                        )
                        .child(render_switch_row(
                            "sftp-preserve-timestamps",
                            "保留时间戳",
                            sftp.preserve_timestamps,
                            state.clone(),
                            |s, v| s.settings.sftp.preserve_timestamps = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-resume-transfers",
                            "断点续传",
                            sftp.resume_transfers,
                            state.clone(),
                            |s, v| s.settings.sftp.resume_transfers = v,
                            cx,
                        )),
                ),
        )
        // 编辑器
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("编辑器", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sftp-builtin-editor",
                            "使用内置编辑器",
                            sftp.use_builtin_editor,
                            state.clone(),
                            |s, v| s.settings.sftp.use_builtin_editor = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-syntax-highlight",
                            "语法高亮",
                            sftp.syntax_highlighting,
                            state.clone(),
                            |s, v| s.settings.sftp.syntax_highlighting = v,
                            cx,
                        )),
                ),
        )
}

/// 渲染监控设置面板
fn render_monitor_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let monitor = &state_read.settings.monitor;

    // 获取输入状态
    let history_retention_input = state_read.history_retention_input.clone();
    let cpu_threshold_input = state_read.cpu_threshold_input.clone();
    let memory_threshold_input = state_read.memory_threshold_input.clone();
    let disk_threshold_input = state_read.disk_threshold_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 数据采集
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("数据采集", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(
                            history_retention_input
                                .as_ref()
                                .map(|input| render_number_row("历史保留(分钟)", input, cx)),
                        )
                        .child(render_switch_row(
                            "monitor-auto-deploy",
                            "自动部署Agent",
                            monitor.auto_deploy_agent,
                            state.clone(),
                            |s, v| s.settings.monitor.auto_deploy_agent = v,
                            cx,
                        )),
                ),
        )
        // 显示项目
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("显示项目", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "monitor-show-cpu",
                            "CPU",
                            monitor.show_cpu,
                            state.clone(),
                            |s, v| s.settings.monitor.show_cpu = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-memory",
                            "内存",
                            monitor.show_memory,
                            state.clone(),
                            |s, v| s.settings.monitor.show_memory = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-disk",
                            "磁盘",
                            monitor.show_disk,
                            state.clone(),
                            |s, v| s.settings.monitor.show_disk = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-network",
                            "网络",
                            monitor.show_network,
                            state.clone(),
                            |s, v| s.settings.monitor.show_network = v,
                            cx,
                        )),
                ),
        )
        // 告警
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("告警阈值", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(
                            cpu_threshold_input
                                .as_ref()
                                .map(|input| render_number_row("CPU (%)", input, cx)),
                        )
                        .children(
                            memory_threshold_input
                                .as_ref()
                                .map(|input| render_number_row("内存 (%)", input, cx)),
                        )
                        .children(
                            disk_threshold_input
                                .as_ref()
                                .map(|input| render_number_row("磁盘 (%)", input, cx)),
                        ),
                ),
        )
}

/// 渲染连接设置面板
fn render_connection_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let conn = &state_read.settings.connection;

    // 获取输入状态
    let default_port_input = state_read.default_port_input.clone();
    let connection_timeout_input = state_read.connection_timeout_input.clone();
    let keepalive_interval_input = state_read.keepalive_interval_input.clone();
    let reconnect_attempts_input = state_read.reconnect_attempts_input.clone();
    let reconnect_interval_input = state_read.reconnect_interval_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // SSH 设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("SSH 设置", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(
                            default_port_input
                                .as_ref()
                                .map(|input| render_number_row("默认端口", input, cx)),
                        )
                        .children(
                            connection_timeout_input
                                .as_ref()
                                .map(|input| render_number_row("连接超时(秒, cx)", input, cx)),
                        )
                        .children(
                            keepalive_interval_input
                                .as_ref()
                                .map(|input| render_number_row("心跳间隔(秒, cx)", input, cx)),
                        )
                        .child(render_switch_row(
                            "conn-compression",
                            "启用压缩",
                            conn.compression,
                            state.clone(),
                            |s, v| s.settings.connection.compression = v,
                            cx,
                        )),
                ),
        )
        // 自动重连
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("自动重连", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_switch_row(
                            "conn-auto-reconnect",
                            "自动重连",
                            conn.auto_reconnect,
                            state.clone(),
                            |s, v| s.settings.connection.auto_reconnect = v,
                            cx,
                        ))
                        .children(
                            reconnect_attempts_input
                                .as_ref()
                                .map(|input| render_number_row("重连次数", input, cx)),
                        )
                        .children(
                            reconnect_interval_input
                                .as_ref()
                                .map(|input| render_number_row("重连间隔(秒, cx)", input, cx)),
                        ),
                ),
        )
}

/// 渲染数据同步面板
fn render_sync_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let sync = &state_read.settings.sync;

    // 获取输入状态
    let webdav_url_input = state_read.webdav_url_input.clone();
    let webdav_path_input = state_read.webdav_path_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 同步状态
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("同步状态", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sync-enabled",
                            "启用同步",
                            sync.enabled,
                            state.clone(),
                            |s, v| s.settings.sync.enabled = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-auto",
                            "自动同步",
                            sync.auto_sync,
                            state.clone(),
                            |s, v| s.settings.sync.auto_sync = v,
                            cx,
                        )),
                ),
        )
        // 同步内容
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("同步内容", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sync-servers",
                            "服务器配置",
                            sync.sync_servers,
                            state.clone(),
                            |s, v| s.settings.sync.sync_servers = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-groups",
                            "分组信息",
                            sync.sync_groups,
                            state.clone(),
                            |s, v| s.settings.sync.sync_groups = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-settings",
                            "应用设置",
                            sync.sync_settings,
                            state.clone(),
                            |s, v| s.settings.sync.sync_settings = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-keybindings",
                            "快捷键",
                            sync.sync_keybindings,
                            state.clone(),
                            |s, v| s.settings.sync.sync_keybindings = v,
                            cx,
                        )),
                ),
        )
        // WebDAV
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("WebDAV 配置", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(
                            webdav_url_input
                                .as_ref()
                                .map(|input| render_input_row("服务器地址", input, cx)),
                        )
                        .children(
                            webdav_path_input
                                .as_ref()
                                .map(|input| render_input_row("同步路径", input, cx)),
                        ),
                ),
        )
}

/// 渲染系统配置面板
fn render_system_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let system = &state_read.settings.system;

    // 获取输入状态
    let log_retention_input = state_read.log_retention_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 启动
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("启动", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-launch-login",
                            "开机启动",
                            system.launch_at_login,
                            state.clone(),
                            |s, v| s.settings.system.launch_at_login = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-start-minimized",
                            "启动时最小化",
                            system.start_minimized,
                            state.clone(),
                            |s, v| s.settings.system.start_minimized = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-check-updates",
                            "检查更新",
                            system.check_updates,
                            state.clone(),
                            |s, v| s.settings.system.check_updates = v,
                            cx,
                        )),
                ),
        )
        // 窗口
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("窗口", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-close-tray",
                            "关闭到托盘",
                            system.close_to_tray,
                            state.clone(),
                            |s, v| s.settings.system.close_to_tray = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-show-tray",
                            "显示托盘图标",
                            system.show_tray_icon,
                            state.clone(),
                            |s, v| s.settings.system.show_tray_icon = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-single-instance",
                            "单实例运行",
                            system.single_instance,
                            state.clone(),
                            |s, v| s.settings.system.single_instance = v,
                            cx,
                        )),
                ),
        )
        // 通知
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("通知", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-notify-disconnect",
                            "断开连接通知",
                            system.notify_on_disconnect,
                            state.clone(),
                            |s, v| s.settings.system.notify_on_disconnect = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-notify-transfer",
                            "传输完成通知",
                            system.notify_on_transfer,
                            state.clone(),
                            |s, v| s.settings.system.notify_on_transfer = v,
                            cx,
                        )),
                ),
        )
        // 日志
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("日志", cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_switch_row(
                            "sys-logging",
                            "启用日志",
                            system.logging_enabled,
                            state.clone(),
                            |s, v| s.settings.system.logging_enabled = v,
                            cx,
                        ))
                        .children(
                            log_retention_input
                                .as_ref()
                                .map(|input| render_number_row("日志保留(天, cx)", input, cx)),
                        ),
                ),
        )
}

// ======================== 辅助渲染函数 ========================

fn render_section_title(title: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_base()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(title)
}

/// 渲染带输入框的设置行（用于文本输入）
fn render_input_row(label: &'static str, input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let text_color = cx.theme().foreground;

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(text_color)
                .child(label),
        )
        .child(div().w(px(200.)).child(Input::new(input).appearance(true)))
}

/// 渲染字体输入框（带下拉选择按钮）
fn render_font_input_row(
    cx: &App,
    label: &'static str,
    input: &Entity<InputState>,
    fonts: &[&'static str],
) -> impl IntoElement {
    use gpui::Corner;

    let input_clone = input.clone();
    let current_value = input.read(cx).value().to_string();
    let fonts = fonts.to_vec();
    let fonts_clone = fonts.clone();

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            // 使用全宽按钮作为下拉触发器，anchor 设为 TopLeft 以便菜单在正下方显示
            Button::new("font-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start() // 内容左对齐
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between() // 两端对齐：文字左侧，图标右侧
                        .w(px(180.)) // 明确设置宽度，减去按钮内边距
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current_value),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(200.));
                    for font in &fonts_clone {
                        let font_name: SharedString = (*font).into();
                        let input_for_click = input_clone.clone();
                        let font_val = font.to_string();
                        menu = menu.item(PopupMenuItem::new(font_name).on_click(
                            move |_, window, cx| {
                                input_for_click.update(cx, |state, cx| {
                                    state.set_value(font_val.clone(), window, cx);
                                });
                            },
                        ));
                    }
                    menu
                }),
        )
}

/// 常用界面字体
const UI_FONTS: &[&str] = &[
    "PingFang SC",
    "SF Pro",
    "Helvetica Neue",
    "Microsoft YaHei",
    "Source Han Sans SC",
    "Noto Sans SC",
    "Arial",
    "system-ui",
];

/// 常用终端等宽字体
const TERMINAL_FONTS: &[&str] = &[
    "JetBrains Mono",
    "Fira Code",
    "SF Mono",
    "Menlo",
    "Consolas",
    "Monaco",
    "Source Code Pro",
    "Hack",
    "IBM Plex Mono",
];

/// 常用终端主题
const TERMINAL_THEMES: &[&str] = &[
    "One Dark",
    "Dracula",
    "Solarized Dark",
    "Solarized Light",
    "Nord",
    "Monokai",
    "Gruvbox Dark",
    "Tokyo Night",
    "GitHub Dark",
];

/// 渲染主题选择行（带下拉菜单）
fn render_theme_select_row(
    label: &'static str,
    current_value: &str,
    themes: &'static [&'static str],
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;

    let current = current_value.to_string();

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            // 使用全宽按钮作为下拉触发器，anchor 设为 TopLeft 以便菜单在正下方显示
            Button::new("theme-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start() // 内容左对齐
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between() // 两端对齐：文字左侧，图标右侧
                        .w(px(180.)) // 明确设置宽度，减去按钮内边距
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(200.));
                    for theme in themes {
                        let theme_name: SharedString = (*theme).into();
                        let theme_val = theme.to_string();
                        let state_clone = state.clone();
                        menu =
                            menu.item(PopupMenuItem::new(theme_name).on_click(move |_, _, cx| {
                                state_clone.update(cx, |s, _| {
                                    s.settings.terminal.color_scheme = theme_val.clone();
                                    s.mark_changed();
                                });
                            }));
                    }
                    menu
                }),
        )
}

/// 渲染带数字输入框的设置行（带 +/- 按钮）

fn render_number_row(
    label: &'static str,
    input: &Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .w(px(200.))
                .flex()
                .justify_end() // 靠右对齐
                .child(NumberInput::new(input).appearance(true)),
        )
}

/// 渲染带开关的设置行
fn render_switch_row(
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    state: Entity<SettingsDialogState>,
    update_fn: fn(&mut SettingsDialogState, bool),
    cx: &App,
) -> impl IntoElement {
    let text_color = cx.theme().foreground;

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(div().text_sm().text_color(text_color).child(label))
        .child(
            Switch::new(id)
                .checked(checked)
                .on_click(move |new_val, _, cx| {
                    state.update(cx, |s, _| {
                        update_fn(s, *new_val);
                        s.mark_changed();
                    });
                }),
        )
}

fn render_about_row(label: &'static str, value: &'static str, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(value),
        )
}
