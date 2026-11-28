// 子模块声明
pub mod helpers;
pub mod panels;

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::theme::{Theme as GpuiTheme, ThemeMode as GpuiThemeMode};
use std::collections::HashMap;

use crate::components::common::button::{text_button, ButtonKind};
use crate::components::common::dialog::{
    dialog_nav_item, dialog_nav_list, dialog_pane_footer, dialog_pane_header, dialog_panel,
    dialog_scroll_area, dialog_sidebar, fit_to_viewport, keep_focus_in_dialog, DialogOverlay,
};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::{
    AiProviderId, ApiFormat, AppSettings, CustomProvider, ProviderRef, ThemeMode,
};
use crate::services::storage;
use crate::terminal::{hex_to_hsla, hsla_to_hex};

// 导入辅助函数
use helpers::create_float_number_input;
use helpers::create_int_number_input;
use helpers::create_text_input;

// 导入面板函数
use panels::{
    render_about_panel, render_ai_panel, render_connection_panel, render_keybindings_panel,
    render_monitor_panel, render_sftp_panel, render_sync_panel, render_system_panel,
    render_terminal_panel, render_theme_panel,
};

/// AI 供应商连通性测试状态
#[derive(Clone, Debug)]
pub enum AiTestStatus {
    Untested,
    Testing,
    Pass,
    Fail(String),
}

/// 单个 AI 供应商在弹窗内的输入控件集合
#[derive(Default)]
pub struct AiProviderInputs {
    pub api_key: Option<Entity<InputState>>,
    pub base_url: Option<Entity<InputState>>,
    /// 已添加的模型列表（首项为默认模型）
    pub models: Option<Vec<String>>,
    /// 手动输入添加模型的单行输入框
    pub new_model_input: Option<Entity<InputState>>,
    /// 自定义供应商：名称输入
    pub name: Option<Entity<InputState>>,
    /// 协议格式（下拉选择）
    pub format: Option<ApiFormat>,
}

/// 从输入框收集到的 AI 供应商值（内置/自定义通用）
pub struct AiInputValues {
    pub name: String,
    pub format: ApiFormat,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
}

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
    Ai,
    About,
}

impl SettingsSection {
    /// 导航中的显示顺序
    pub const ALL: [SettingsSection; 10] = [
        SettingsSection::Theme,
        SettingsSection::Terminal,
        SettingsSection::KeyBindings,
        SettingsSection::Sftp,
        SettingsSection::Monitor,
        SettingsSection::Connection,
        SettingsSection::Sync,
        SettingsSection::Ai,
        SettingsSection::System,
        SettingsSection::About,
    ];

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
            SettingsSection::Ai => "settings.nav.ai",
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
            SettingsSection::Ai => icons::SPARKLES,
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
    /// 打开弹窗时的设置。取消时据此撤销已经即时生效的改动（主题模式、语言）
    saved_settings: Option<AppSettings>,
    /// 保存到磁盘失败时的错误信息
    pub save_error: Option<String>,
    /// 弹窗的焦点：用于接收 Esc；打开时聚焦
    pub focus_handle: Option<FocusHandle>,
    focus_pending: bool,
    /// 内容区的滚动位置，切换分区时回到顶部
    pub scroll_handle: ScrollHandle,

    // ============ 主题设置输入 ============
    pub ui_font_family_input: Option<Entity<InputState>>,
    pub ui_font_size_input: Option<Entity<InputState>>,

    // ============ 终端设置输入 ============
    pub terminal_font_family_input: Option<Entity<InputState>>,
    pub terminal_font_size_input: Option<Entity<InputState>>,
    pub terminal_line_height_input: Option<Entity<InputState>>,
    pub scrollback_lines_input: Option<Entity<InputState>>,
    pub terminal_blink_interval_input: Option<Entity<InputState>>,
    pub terminal_background_opacity_input: Option<Entity<InputState>>,
    pub terminal_padding_input: Option<Entity<InputState>>,
    pub terminal_scroll_multiplier_input: Option<Entity<InputState>>,
    /// 配色：前景/背景/光标/选区颜色选择器（变更时通过事件实时写入 settings）
    pub terminal_foreground_picker: Option<Entity<ColorPickerState>>,
    pub terminal_background_picker: Option<Entity<ColorPickerState>>,
    pub terminal_cursor_picker: Option<Entity<ColorPickerState>>,
    pub terminal_selection_picker: Option<Entity<ColorPickerState>>,
    pub terminal_word_separators_input: Option<Entity<InputState>>,
    pub terminal_term_type_input: Option<Entity<InputState>>,
    pub terminal_default_shell_input: Option<Entity<InputState>>,
    pub terminal_shell_args_input: Option<Entity<InputState>>,

    // ============ 连接设置输入 ============
    pub default_port_input: Option<Entity<InputState>>,
    pub connection_timeout_input: Option<Entity<InputState>>,
    pub keepalive_interval_input: Option<Entity<InputState>>,
    pub reconnect_attempts_input: Option<Entity<InputState>>,
    pub reconnect_interval_input: Option<Entity<InputState>>,
    pub global_proxy_host_input: Option<Entity<InputState>>,
    pub global_proxy_port_input: Option<Entity<InputState>>,
    pub global_proxy_username_input: Option<Entity<InputState>>,
    pub global_proxy_password_input: Option<Entity<InputState>>,

    // ============ 监控设置输入 ============
    pub history_retention_input: Option<Entity<InputState>>,
    pub cpu_threshold_input: Option<Entity<InputState>>,
    pub memory_threshold_input: Option<Entity<InputState>>,
    pub disk_threshold_input: Option<Entity<InputState>>,

    // ============ SFTP 设置输入 ============
    pub concurrent_transfers_input: Option<Entity<InputState>>,
    pub local_default_path_input: Option<Entity<InputState>>,
    // 编辑器设置输入
    pub external_editor_path_input: Option<Entity<InputState>>,
    pub max_edit_file_size_input: Option<Entity<InputState>>,
    pub editor_font_family_input: Option<Entity<InputState>>,
    pub editor_font_size_input: Option<Entity<InputState>>,
    pub editor_line_height_input: Option<Entity<InputState>>,
    pub editor_gutter_width_input: Option<Entity<InputState>>,
    pub editor_gutter_padding_input: Option<Entity<InputState>>,

    // ============ 同步设置输入 ============
    pub webdav_url_input: Option<Entity<InputState>>,
    pub webdav_username_input: Option<Entity<InputState>>,
    pub webdav_password_input: Option<Entity<InputState>>,
    pub webdav_path_input: Option<Entity<InputState>>,
    pub webdav_passphrase_input: Option<Entity<InputState>>,
    /// 正在测试 WebDAV 连通性
    pub sync_testing: bool,
    /// 正在执行 WebDAV 同步
    pub sync_running: bool,
    /// WebDAV 测试或同步的结果提示 (is_ok, message)
    pub sync_notice: Option<(bool, String)>,
    /// 同步完成后标记给首页刷新服务器/片段列表
    pub sync_data_updated: bool,

    // ============ 系统设置输入 ============
    pub log_retention_input: Option<Entity<InputState>>,
    /// 全局自动更新状态（与 HomePage / 侧边栏共享）
    pub update_state: Option<Entity<crate::state::UpdateState>>,

    // ============ AI 设置 ============
    pub ai_inputs: HashMap<ProviderRef, AiProviderInputs>,
    pub ai_test_statuses: HashMap<ProviderRef, AiTestStatus>,
    /// 正在从远端 `/models` 拉取模型列表的供应商集合
    pub ai_fetching_models: std::collections::HashSet<ProviderRef>,
    /// 从远端 `/models` 拉取到的候选模型池（供下拉选择添加）
    pub ai_remote_models: HashMap<ProviderRef, Vec<String>>,
    /// 最近一次拉取模型列表的状态提示 (is_ok, message)
    pub ai_fetch_notices: HashMap<ProviderRef, (bool, String)>,
    /// 已通过测试时的输入快照 (api_key, base_url, model, format_tag)，用于检测用户在通过后是否又改了
    pub ai_tested_snapshots: HashMap<ProviderRef, (String, String, String, String)>,
    /// 保存校验失败时的错误信息（如某供应商有 key 但未通过测试）
    pub ai_save_error: Option<String>,
    /// AI 面板当前编辑/查看的供应商（横向图标切换）
    pub ai_active_provider: ProviderRef,
    /// 系统提示词输入框
    pub ai_system_prompt_input: Option<Entity<TextareaState>>,
    /// 终端上下文：携带行数输入框
    pub ai_context_lines_input: Option<Entity<InputState>>,
    /// Agent：单次最大工具轮数输入框
    pub ai_agent_max_turns_input: Option<Entity<InputState>>,
    /// Agent：单条命令超时（秒）输入框
    pub ai_agent_timeout_input: Option<Entity<InputState>>,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        let settings = storage::load_settings().unwrap_or_default();
        Self {
            visible: false,
            current_section: SettingsSection::Theme,
            settings,
            has_changes: false,
            saved_settings: None,
            save_error: None,
            focus_handle: None,
            focus_pending: false,
            scroll_handle: ScrollHandle::new(),
            // 主题
            ui_font_family_input: None,
            ui_font_size_input: None,
            // 终端
            terminal_font_family_input: None,
            terminal_font_size_input: None,
            terminal_line_height_input: None,
            scrollback_lines_input: None,
            terminal_blink_interval_input: None,
            terminal_background_opacity_input: None,
            terminal_padding_input: None,
            terminal_scroll_multiplier_input: None,
            terminal_foreground_picker: None,
            terminal_background_picker: None,
            terminal_cursor_picker: None,
            terminal_selection_picker: None,
            terminal_word_separators_input: None,
            terminal_term_type_input: None,
            terminal_default_shell_input: None,
            terminal_shell_args_input: None,
            // 连接
            default_port_input: None,
            connection_timeout_input: None,
            keepalive_interval_input: None,
            reconnect_attempts_input: None,
            reconnect_interval_input: None,
            global_proxy_host_input: None,
            global_proxy_port_input: None,
            global_proxy_username_input: None,
            global_proxy_password_input: None,
            // 监控
            history_retention_input: None,
            cpu_threshold_input: None,
            memory_threshold_input: None,
            disk_threshold_input: None,
            // SFTP
            concurrent_transfers_input: None,
            local_default_path_input: None,
            external_editor_path_input: None,
            max_edit_file_size_input: None,
            editor_font_family_input: None,
            editor_font_size_input: None,
            editor_line_height_input: None,
            editor_gutter_width_input: None,
            editor_gutter_padding_input: None,
            // 同步
            webdav_url_input: None,
            webdav_username_input: None,
            webdav_password_input: None,
            webdav_path_input: None,
            webdav_passphrase_input: None,
            sync_testing: false,
            sync_running: false,
            sync_notice: None,
            sync_data_updated: false,
            // 系统
            log_retention_input: None,
            update_state: None,
            // AI
            ai_inputs: HashMap::new(),
            ai_test_statuses: HashMap::new(),
            ai_fetching_models: std::collections::HashSet::new(),
            ai_remote_models: HashMap::new(),
            ai_fetch_notices: HashMap::new(),
            ai_tested_snapshots: HashMap::new(),
            ai_save_error: None,
            ai_active_provider: ProviderRef::Builtin(AiProviderId::OpenAi),
            ai_system_prompt_input: None,
            ai_context_lines_input: None,
            ai_agent_max_turns_input: None,
            ai_agent_timeout_input: None,
        }
    }
}

impl SettingsDialogState {
    pub fn with_updater(update_state: Entity<crate::state::UpdateState>) -> Self {
        Self {
            update_state: Some(update_state),
            ..Self::default()
        }
    }

    pub fn open_section(&mut self, section: SettingsSection) {
        self.open();
        self.current_section = section;
    }

    pub fn open(&mut self) {
        // 打开时重新加载设置
        self.settings = storage::load_settings().unwrap_or_default();
        self.saved_settings = Some(self.settings.clone());
        self.visible = true;
        self.current_section = SettingsSection::Theme;
        self.has_changes = false;
        self.ai_save_error = None;
        self.save_error = None;
        self.focus_pending = true;
        self.scroll_handle.set_offset(Point::default());
        // 清除输入状态以便重新加载
        self.reset_inputs();
        self.ai_fetching_models.clear();
        self.ai_remote_models.clear();
        self.ai_fetch_notices.clear();
        self.refresh_ai_verified_snapshots();
    }

    /// 根据当前 `self.settings` 中持久化的 verified 标志，重建 AI 测试状态与快照
    fn refresh_ai_verified_snapshots(&mut self) {
        self.ai_test_statuses.clear();
        self.ai_tested_snapshots.clear();
        for id in AiProviderId::ALL {
            let r = ProviderRef::Builtin(id);
            let cfg = self.settings.ai.get(id);
            let has_credential = !id.requires_api_key() || !cfg.api_key.trim().is_empty();
            if cfg.verified && has_credential {
                self.ai_test_statuses.insert(r.clone(), AiTestStatus::Pass);
                self.ai_tested_snapshots.insert(
                    r,
                    (
                        cfg.api_key.clone(),
                        cfg.base_url.clone(),
                        cfg.model.clone(),
                        cfg.effective_format(id).label().to_string(),
                    ),
                );
            } else {
                self.ai_test_statuses.insert(r, AiTestStatus::Untested);
            }
        }
        for c in &self.settings.ai.custom_providers {
            let r = ProviderRef::Custom(c.id.clone());
            let has_credential = crate::models::settings::is_local_base_url(&c.base_url)
                || !c.api_key.trim().is_empty();
            if c.verified && has_credential {
                self.ai_test_statuses.insert(r.clone(), AiTestStatus::Pass);
                self.ai_tested_snapshots.insert(
                    r,
                    (
                        c.api_key.clone(),
                        c.base_url.clone(),
                        c.model.clone(),
                        c.format.label().to_string(),
                    ),
                );
            } else {
                self.ai_test_statuses.insert(r, AiTestStatus::Untested);
            }
        }
    }

    /// 手动「立即同步」成功后回填合并后的最新配置，防止随后点「保存」时用旧快照覆盖或触发二次自动同步
    pub fn reload_after_sync(&mut self) {
        if let Ok(fresh) = storage::load_settings() {
            self.settings = fresh;
            self.saved_settings = Some(self.settings.clone());
            self.has_changes = false;
            self.ai_inputs.clear();
            self.ai_system_prompt_input = None;
            self.ai_context_lines_input = None;
            self.ai_agent_max_turns_input = None;
            self.ai_agent_timeout_input = None;
            self.refresh_ai_verified_snapshots();
        }
    }

    /// 重置所有输入框状态
    fn reset_inputs(&mut self) {
        self.ui_font_family_input = None;
        self.ui_font_size_input = None;
        self.terminal_font_family_input = None;
        self.terminal_font_size_input = None;
        self.terminal_line_height_input = None;
        self.scrollback_lines_input = None;
        self.terminal_blink_interval_input = None;
        self.terminal_background_opacity_input = None;
        self.terminal_padding_input = None;
        self.terminal_scroll_multiplier_input = None;
        self.terminal_foreground_picker = None;
        self.terminal_background_picker = None;
        self.terminal_cursor_picker = None;
        self.terminal_selection_picker = None;
        self.terminal_word_separators_input = None;
        self.terminal_term_type_input = None;
        self.terminal_default_shell_input = None;
        self.terminal_shell_args_input = None;
        self.default_port_input = None;
        self.connection_timeout_input = None;
        self.keepalive_interval_input = None;
        self.reconnect_attempts_input = None;
        self.reconnect_interval_input = None;
        self.global_proxy_host_input = None;
        self.global_proxy_port_input = None;
        self.global_proxy_username_input = None;
        self.global_proxy_password_input = None;
        self.history_retention_input = None;
        self.cpu_threshold_input = None;
        self.memory_threshold_input = None;
        self.disk_threshold_input = None;
        self.concurrent_transfers_input = None;
        self.local_default_path_input = None;
        self.external_editor_path_input = None;
        self.max_edit_file_size_input = None;
        self.editor_font_family_input = None;
        self.editor_font_size_input = None;
        self.editor_line_height_input = None;
        self.editor_gutter_width_input = None;
        self.editor_gutter_padding_input = None;
        self.webdav_url_input = None;
        self.webdav_username_input = None;
        self.webdav_password_input = None;
        self.webdav_path_input = None;
        self.webdav_passphrase_input = None;
        self.sync_testing = false;
        self.sync_running = false;
        self.sync_notice = None;
        self.log_retention_input = None;
        self.ai_inputs.clear();
        self.ai_system_prompt_input = None;
        self.ai_context_lines_input = None;
        self.ai_agent_max_turns_input = None;
        self.ai_agent_timeout_input = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.saved_settings = None;
    }

    /// 放弃本次修改并关闭：撤销已经即时生效的主题模式，内存中的设置（包括语言）恢复为打开时的值
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(saved) = self.saved_settings.take() {
            if saved.theme.mode != self.settings.theme.mode {
                apply_theme_mode(&saved.theme.mode, window, cx);
            }
            self.settings = saved;
        }
        self.close();
        cx.notify();
    }

    /// 校验并保存；成功后关闭，失败时把原因留在弹窗里显示
    pub fn try_save(&mut self, cx: &mut Context<Self>) {
        // AI 设置校验：填了 key 必须先通过测试
        if let Err(failing) = self.validate_ai_for_save(cx) {
            let lang = &self.settings.theme.language;
            self.ai_save_error = Some(format!(
                "{}: {}",
                i18n::t(lang, "settings.ai.save_blocked"),
                failing.join(", ")
            ));
            self.select_section(SettingsSection::Ai);
            cx.notify();
            return;
        }
        self.ai_save_error = None;
        self.sync_from_inputs(cx);
        match self.save() {
            Ok(()) => self.close(),
            Err(e) => {
                let lang = &self.settings.theme.language;
                self.save_error = Some(format!("{}{:#}", i18n::t(lang, "settings.save_failed"), e));
            }
        }
        cx.notify();
    }

    /// 在手动「立即同步」前将弹窗中已通过校验的其他设置落盘（不触发后台自动同步，由当前手动同步一并推送）
    pub fn flush_before_manual_sync(&mut self, cx: &mut Context<Self>) {
        if self.validate_ai_for_save(cx).is_err() {
            return;
        }
        self.sync_from_inputs(cx);
        let disk_current = storage::cached_settings();
        if Self::settings_equal_ignoring_meta(&self.settings, &disk_current) {
            return;
        }
        if !Self::settings_equal_ignoring_meta_and_sync(&self.settings, &disk_current) {
            self.settings.updated_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = storage::save_settings(&self.settings);
    }

    fn settings_equal_ignoring_meta(a: &AppSettings, b: &AppSettings) -> bool {
        let mut na = a.clone();
        let mut nb = b.clone();
        na.updated_at = None;
        nb.updated_at = None;
        na.sync.last_sync_at = None;
        nb.sync.last_sync_at = None;
        match (serde_json::to_value(&na), serde_json::to_value(&nb)) {
            (Ok(va), Ok(vb)) => va == vb,
            _ => false,
        }
    }

    fn settings_equal_ignoring_meta_and_sync(a: &AppSettings, b: &AppSettings) -> bool {
        let mut nb = b.clone();
        nb.sync = a.sync.clone();
        Self::settings_equal_ignoring_meta(a, &nb)
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        let disk_current = storage::cached_settings();
        if Self::settings_equal_ignoring_meta(&self.settings, &disk_current) {
            self.has_changes = false;
            self.save_error = None;
            return Ok(());
        }
        let non_sync_changed =
            !Self::settings_equal_ignoring_meta_and_sync(&self.settings, &disk_current);
        if non_sync_changed {
            self.settings.updated_at = Some(chrono::Utc::now().to_rfc3339());
        }
        storage::save_settings(&self.settings)?;
        if non_sync_changed {
            crate::services::sync::notify_local_data_changed();
        }
        self.has_changes = false;
        self.save_error = None;
        Ok(())
    }

    /// 切换分区，内容滚回顶部
    pub fn select_section(&mut self, section: SettingsSection) {
        self.current_section = section;
        self.scroll_handle.set_offset(Point::default());
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
        if self.terminal_blink_interval_input.is_none() {
            let value = self.settings.terminal.cursor_blink_interval_ms.to_string();
            self.terminal_blink_interval_input =
                Some(create_int_number_input(value, 100, 2000, 50, window, cx));
        }
        if self.terminal_background_opacity_input.is_none() {
            let value = self.settings.terminal.background_opacity.to_string();
            self.terminal_background_opacity_input =
                Some(create_int_number_input(value, 0, 100, 5, window, cx));
        }
        if self.terminal_padding_input.is_none() {
            let value = self.settings.terminal.padding.to_string();
            self.terminal_padding_input =
                Some(create_int_number_input(value, 0, 32, 1, window, cx));
        }
        if self.terminal_scroll_multiplier_input.is_none() {
            let value = format!("{:.1}", self.settings.terminal.scroll_multiplier);
            self.terminal_scroll_multiplier_input =
                Some(create_float_number_input(value, 0.1, 10.0, 0.1, window, cx));
        }

        // 颜色选择器：选色/回车确认后通过事件实时写入 settings（预览卡片与色块标签直接读取 settings），
        // 保存时不再从控件回读，避免未修改的颜色被 hex↔hsla 转换改写。
        macro_rules! ensure_color_picker {
            ($field:ident, $setting:ident) => {
                if self.$field.is_none() {
                    let initial = hex_to_hsla(&self.settings.terminal.$setting);
                    let picker =
                        cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial));
                    cx.subscribe_in(
                        &picker,
                        window,
                        |this, _, event: &ColorPickerEvent, _, cx| {
                            if let ColorPickerEvent::Change(Some(color)) = event {
                                this.settings.terminal.$setting = hsla_to_hex(*color);
                                this.mark_changed();
                                cx.notify();
                            }
                        },
                    )
                    .detach();
                    self.$field = Some(picker);
                }
            };
        }
        ensure_color_picker!(terminal_foreground_picker, foreground_color);
        ensure_color_picker!(terminal_background_picker, background_color);
        ensure_color_picker!(terminal_cursor_picker, cursor_color);
        ensure_color_picker!(terminal_selection_picker, selection_color);

        macro_rules! ensure_terminal_text_input {
            ($field:ident, $value:expr) => {
                if self.$field.is_none() {
                    let value = $value;
                    self.$field = Some(cx.new(|cx| {
                        let mut state = InputState::new(window, cx);
                        state.set_value(value, window, cx);
                        state
                    }));
                }
            };
        }
        ensure_terminal_text_input!(
            terminal_word_separators_input,
            self.settings.terminal.word_separators.clone()
        );
        ensure_terminal_text_input!(
            terminal_term_type_input,
            self.settings.terminal.term_type.clone()
        );
        ensure_terminal_text_input!(
            terminal_default_shell_input,
            self.settings.terminal.default_shell.clone()
        );
        ensure_terminal_text_input!(
            terminal_shell_args_input,
            self.settings.terminal.shell_args.clone()
        );

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
        // 全局代理
        let conn = self.settings.connection.clone();
        let lang = self.settings.theme.language.clone();
        if self.global_proxy_host_input.is_none() {
            self.global_proxy_host_input = Some(create_text_input(
                conn.global_proxy_host.clone(),
                i18n::t(&lang, "settings.connection.global_proxy_host"),
                false,
                window,
                cx,
            ));
        }
        if self.global_proxy_port_input.is_none() {
            let value = if conn.global_proxy_port == 0 {
                String::new()
            } else {
                conn.global_proxy_port.to_string()
            };
            self.global_proxy_port_input =
                Some(create_text_input(value, "1080", false, window, cx));
        }
        if self.global_proxy_username_input.is_none() {
            self.global_proxy_username_input = Some(create_text_input(
                conn.global_proxy_username.clone(),
                i18n::t(&lang, "settings.connection.global_proxy_username"),
                false,
                window,
                cx,
            ));
        }
        if self.global_proxy_password_input.is_none() {
            self.global_proxy_password_input = Some(create_text_input(
                conn.global_proxy_password.clone(),
                i18n::t(&lang, "settings.connection.global_proxy_password"),
                true,
                window,
                cx,
            ));
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
        // 编辑器设置
        if self.external_editor_path_input.is_none() {
            let value = self.settings.sftp.external_editor_path.clone();
            let placeholder = i18n::t(lang, "settings.sftp.external_editor_placeholder");
            self.external_editor_path_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.max_edit_file_size_input.is_none() {
            let value = self.settings.sftp.max_edit_file_size_kb.to_string();
            self.max_edit_file_size_input =
                Some(create_int_number_input(value, 1, 102400, 1, window, cx)); // 1KB - 100MB
        }
        if self.editor_font_family_input.is_none() {
            let value = self.settings.sftp.editor_font_family.clone();
            self.editor_font_family_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            }));
        }
        if self.editor_font_size_input.is_none() {
            let value = self.settings.sftp.editor_font_size.to_string();
            self.editor_font_size_input =
                Some(create_int_number_input(value, 8, 72, 1, window, cx));
        }
        if self.editor_line_height_input.is_none() {
            let value = format!("{:.1}", self.settings.sftp.editor_line_height);
            self.editor_line_height_input =
                Some(create_float_number_input(value, 1.0, 3.0, 0.1, window, cx));
        }
        if self.editor_gutter_width_input.is_none() {
            let value = self.settings.sftp.editor_gutter_width.to_string();
            self.editor_gutter_width_input =
                Some(create_int_number_input(value, 20, 100, 1, window, cx));
        }
        if self.editor_gutter_padding_input.is_none() {
            let value = self.settings.sftp.editor_gutter_padding.to_string();
            self.editor_gutter_padding_input =
                Some(create_int_number_input(value, 0, 20, 1, window, cx));
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
        if self.webdav_passphrase_input.is_none() {
            let value = self.settings.sync.sync_passphrase.clone();
            let placeholder = i18n::t(lang, "settings.sync.passphrase_placeholder");
            self.webdav_passphrase_input = Some(cx.new(|cx| {
                let mut state = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(true);
                state.set_value(value, window, cx);
                state
            }));
        }

        // 系统设置
        if self.log_retention_input.is_none() {
            let value = self.settings.system.log_retention_days.to_string();
            self.log_retention_input = Some(create_int_number_input(value, 1, 365, 1, window, cx));
        }

        // AI 设置（内置供应商）
        for id in AiProviderId::ALL {
            let r = ProviderRef::Builtin(id);
            let cfg = self.settings.ai.get(id);
            let lang = self.settings.theme.language.clone();
            let needs_model_input = self
                .ai_inputs
                .get(&r)
                .map(|e| e.new_model_input.is_none())
                .unwrap_or(true);
            {
                let entry = self.ai_inputs.entry(r.clone()).or_default();
                if entry.format.is_none() {
                    entry.format = Some(cfg.effective_format(id));
                }
                if entry.api_key.is_none() {
                    let value = cfg.api_key.clone();
                    let placeholder = if id.requires_api_key() {
                        "API Key"
                    } else {
                        i18n::t(&lang, "settings.ai.api_key_optional")
                    };
                    entry.api_key = Some(cx.new(|cx| {
                        let mut state = InputState::new(window, cx)
                            .placeholder(placeholder)
                            .masked(true);
                        state.set_value(value, window, cx);
                        state
                    }));
                }
                if entry.base_url.is_none() {
                    let value = cfg.base_url.clone();
                    let placeholder = id.default_base_url();
                    entry.base_url = Some(cx.new(|cx| {
                        let mut state = InputState::new(window, cx).placeholder(placeholder);
                        state.set_value(value, window, cx);
                        state
                    }));
                }
                if entry.models.is_none() {
                    entry.models = Some(cfg.model_list());
                }
            }
            if needs_model_input {
                let placeholder = i18n::t(&lang, "settings.ai.model_input_placeholder");
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                let r_for_enter = r.clone();
                let input_for_enter = input.clone();
                cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                        if matches!(
                            event,
                            gpui_kit::component::input::InputEvent::PressEnter { .. }
                        ) {
                            let val = input_for_enter.read(cx).value().trim().to_string();
                            if !val.is_empty() {
                                this.add_ai_model(&r_for_enter, val);
                                input_for_enter.update(cx, |s, cx| {
                                    s.set_value(String::new(), window, cx);
                                });
                                cx.notify();
                            }
                        }
                    },
                )
                .detach();
                self.ai_inputs.entry(r).or_default().new_model_input = Some(input);
            }
        }

        // AI 设置（自定义供应商）
        let custom_list = self.settings.ai.custom_providers.clone();
        for c in &custom_list {
            let r = ProviderRef::Custom(c.id.clone());
            let lang = self.settings.theme.language.clone();
            let needs_model_input = self
                .ai_inputs
                .get(&r)
                .map(|e| e.new_model_input.is_none())
                .unwrap_or(true);
            {
                let entry = self.ai_inputs.entry(r.clone()).or_default();
                if entry.format.is_none() {
                    entry.format = Some(c.format);
                }
                if entry.name.is_none() {
                    let value = c.name.clone();
                    entry.name = Some(cx.new(|cx| {
                        let mut state = InputState::new(window, cx)
                            .placeholder(i18n::t(&lang, "settings.ai.name_placeholder"));
                        state.set_value(value, window, cx);
                        state
                    }));
                }
                if entry.api_key.is_none() {
                    let value = c.api_key.clone();
                    entry.api_key = Some(cx.new(|cx| {
                        let mut state = InputState::new(window, cx)
                            .placeholder("API Key")
                            .masked(true);
                        state.set_value(value, window, cx);
                        state
                    }));
                }
                if entry.base_url.is_none() {
                    let value = c.base_url.clone();
                    entry.base_url = Some(cx.new(|cx| {
                        let mut state =
                            InputState::new(window, cx).placeholder("https://api.example.com/v1");
                        state.set_value(value, window, cx);
                        state
                    }));
                }
                if entry.models.is_none() {
                    entry.models = Some(c.model_list());
                }
            }
            if needs_model_input {
                let placeholder = i18n::t(&lang, "settings.ai.model_input_placeholder");
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                let r_for_enter = r.clone();
                let input_for_enter = input.clone();
                cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                        if matches!(
                            event,
                            gpui_kit::component::input::InputEvent::PressEnter { .. }
                        ) {
                            let val = input_for_enter.read(cx).value().trim().to_string();
                            if !val.is_empty() {
                                this.add_ai_model(&r_for_enter, val);
                                input_for_enter.update(cx, |s, cx| {
                                    s.set_value(String::new(), window, cx);
                                });
                                cx.notify();
                            }
                        }
                    },
                )
                .detach();
                self.ai_inputs.entry(r).or_default().new_model_input = Some(input);
            }
        }

        // 系统提示词
        if self.ai_system_prompt_input.is_none() {
            let value = self.settings.ai.system_prompt.clone();
            self.ai_system_prompt_input = Some(cx.new(|cx| {
                let mut state = TextareaState::new(window, cx)
                    .placeholder(i18n::t(&self.settings.theme.language, "settings.ai.system_prompt_placeholder"))
                    .auto_grow(3, 12);
                state.set_value(value, window, cx);
                state
            }));
        }

        // 终端上下文：携带行数
        if self.ai_context_lines_input.is_none() {
            let value = self.settings.ai.context.output_lines.to_string();
            self.ai_context_lines_input =
                Some(create_int_number_input(value, 1, 500, 10, window, cx));
        }

        // Agent：最大工具轮数
        if self.ai_agent_max_turns_input.is_none() {
            let value = self.settings.ai.agent.max_turns.to_string();
            self.ai_agent_max_turns_input =
                Some(create_int_number_input(value, 1, 30, 1, window, cx));
        }

        // Agent：命令超时时间（秒）
        if self.ai_agent_timeout_input.is_none() {
            let value = self.settings.ai.agent.command_timeout_secs.to_string();
            self.ai_agent_timeout_input =
                Some(create_int_number_input(value, 3, 60, 5, window, cx));
        }

        // 弹窗焦点：打开时聚焦，Esc 才能关闭弹窗（否则按键会落到弹窗下面的界面）
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            focus_handle.focus(window, cx);
        }
        keep_focus_in_dialog(cx.weak_entity(), |s: &Self| s.visible, &focus_handle, window);
    }

    /// 从 AI 输入框读取当前值，构造一个 AiProviderConfig（不修改 settings）
    /// 从 AI 输入框读取当前值（内置/自定义通用）
    pub fn collect_ai_inputs(&self, r: &ProviderRef, cx: &App) -> AiInputValues {
        let inputs = self.ai_inputs.get(r);
        // 默认值（用于输入框不存在时回退）
        let (def_name, def_format, def_key, def_url, def_models): (
            String,
            ApiFormat,
            String,
            String,
            Vec<String>,
        ) = match r {
            ProviderRef::Builtin(id) => {
                let cfg = self.settings.ai.get(*id);
                (
                    id.label().to_string(),
                    cfg.effective_format(*id),
                    cfg.api_key.clone(),
                    cfg.base_url.clone(),
                    cfg.model_list(),
                )
            }
            ProviderRef::Custom(cid) => {
                let c = self.settings.ai.custom_get(cid).cloned();
                (
                    c.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
                    c.as_ref().map(|c| c.format).unwrap_or_default(),
                    c.as_ref().map(|c| c.api_key.clone()).unwrap_or_default(),
                    c.as_ref().map(|c| c.base_url.clone()).unwrap_or_default(),
                    c.as_ref().map(|c| c.model_list()).unwrap_or_default(),
                )
            }
        };

        let name = inputs
            .and_then(|i| i.name.as_ref())
            .map(|s| s.read(cx).value().to_string())
            .unwrap_or(def_name);
        let format = inputs.and_then(|i| i.format).unwrap_or(def_format);
        let api_key = inputs
            .and_then(|i| i.api_key.as_ref())
            .map(|s| s.read(cx).value().to_string())
            .unwrap_or(def_key);
        let base_url = inputs
            .and_then(|i| i.base_url.as_ref())
            .map(|s| s.read(cx).value().to_string())
            .unwrap_or(def_url);
        let models: Vec<String> = inputs
            .and_then(|i| i.models.clone())
            .unwrap_or(def_models);
        let model = models.first().cloned().unwrap_or_default();
        AiInputValues {
            name,
            format,
            api_key,
            base_url,
            model,
            models,
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
                self.settings.terminal.scrollback_lines = v.clamp(100, 100_000);
            }
        }
        if let Some(input) = &self.terminal_blink_interval_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.terminal.cursor_blink_interval_ms = v.clamp(100, 2_000);
            }
        }
        if let Some(input) = &self.terminal_background_opacity_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.terminal.background_opacity = v.min(100);
            }
        }
        if let Some(input) = &self.terminal_padding_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.terminal.padding = v.min(32);
            }
        }
        if let Some(input) = &self.terminal_scroll_multiplier_input {
            if let Ok(v) = input.read(cx).value().parse::<f32>() {
                self.settings.terminal.scroll_multiplier = v.clamp(0.1, 10.0);
            }
        }
        macro_rules! sync_terminal_text_input {
            ($field:ident, $target:ident) => {
                if let Some(input) = &self.$field {
                    self.settings.terminal.$target = input.read(cx).value().to_string();
                }
            };
        }
        // 前景/背景/光标/选区颜色由颜色选择器事件实时写入 settings，这里无需回读。
        sync_terminal_text_input!(terminal_word_separators_input, word_separators);
        sync_terminal_text_input!(terminal_term_type_input, term_type);
        sync_terminal_text_input!(terminal_default_shell_input, default_shell);
        sync_terminal_text_input!(terminal_shell_args_input, shell_args);
        if self.settings.terminal.term_type.trim().is_empty() {
            self.settings.terminal.term_type = "xterm-256color".to_string();
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
        if let Some(input) = &self.global_proxy_host_input {
            self.settings.connection.global_proxy_host = input.read(cx).value().trim().to_string();
        }
        if let Some(input) = &self.global_proxy_port_input {
            let value = input.read(cx).value().trim().to_string();
            if value.is_empty() {
                self.settings.connection.global_proxy_port = 0;
            } else if let Ok(v) = value.parse::<u16>() {
                self.settings.connection.global_proxy_port = v;
            }
        }
        if let Some(input) = &self.global_proxy_username_input {
            self.settings.connection.global_proxy_username =
                input.read(cx).value().trim().to_string();
        }
        if let Some(input) = &self.global_proxy_password_input {
            self.settings.connection.global_proxy_password = input.read(cx).value().to_string();
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
        if let Some(input) = &self.concurrent_transfers_input {
            if let Ok(v) = input.read(cx).value().trim().parse::<u32>() {
                self.settings.sftp.concurrent_transfers = v.clamp(1, 10);
            }
        }
        if let Some(input) = &self.local_default_path_input {
            self.settings.sftp.local_default_path = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.external_editor_path_input {
            self.settings.sftp.external_editor_path = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.max_edit_file_size_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.sftp.max_edit_file_size_kb = v;
            }
        }
        if let Some(input) = &self.editor_font_family_input {
            self.settings.sftp.editor_font_family = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.editor_font_size_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.sftp.editor_font_size = v;
            }
        }
        if let Some(input) = &self.editor_line_height_input {
            if let Ok(v) = input.read(cx).value().parse::<f32>() {
                self.settings.sftp.editor_line_height = v;
            }
        }
        if let Some(input) = &self.editor_gutter_width_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.sftp.editor_gutter_width = v;
            }
        }
        if let Some(input) = &self.editor_gutter_padding_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.sftp.editor_gutter_padding = v;
            }
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
        if let Some(input) = &self.webdav_passphrase_input {
            self.settings.sync.sync_passphrase = input.read(cx).value().to_string();
        }

        // 系统
        if let Some(input) = &self.log_retention_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.system.log_retention_days = v;
            }
        }

        // AI - 系统提示词
        if let Some(input) = &self.ai_system_prompt_input {
            self.settings.ai.system_prompt = input.read(cx).value().to_string();
        }
        // AI - 终端上下文携带行数
        if let Some(input) = &self.ai_context_lines_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.ai.context.output_lines = v;
            }
        }
        // AI - Agent 最大工具轮数与命令超时
        if let Some(input) = &self.ai_agent_max_turns_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.ai.agent.max_turns = v.clamp(1, 30);
            }
        }
        if let Some(input) = &self.ai_agent_timeout_input {
            if let Ok(v) = input.read(cx).value().parse::<u32>() {
                self.settings.ai.agent.command_timeout_secs = v.clamp(3, 60);
            }
        }
        // AI - 内置供应商
        for id in AiProviderId::ALL {
            let r = ProviderRef::Builtin(id);
            let v = self.collect_ai_inputs(&r, cx);
            let verified = self.ai_ref_verified(&r, &v);
            let format = (v.format != id.api_format()).then_some(v.format);
            self.settings.ai.set(
                id,
                crate::models::settings::AiProviderConfig {
                    api_key: v.api_key,
                    base_url: v.base_url,
                    format,
                    model: v.model,
                    models: v.models,
                    verified,
                },
            );
        }
        // AI - 自定义供应商（按现有 id 列表重建）
        let custom_ids: Vec<String> = self
            .settings
            .ai
            .custom_providers
            .iter()
            .map(|c| c.id.clone())
            .collect();
        let mut rebuilt: Vec<CustomProvider> = Vec::with_capacity(custom_ids.len());
        for cid in custom_ids {
            let r = ProviderRef::Custom(cid.clone());
            let v = self.collect_ai_inputs(&r, cx);
            let verified = self.ai_ref_verified(&r, &v);
            rebuilt.push(CustomProvider {
                id: cid,
                name: v.name,
                format: v.format,
                api_key: v.api_key,
                base_url: v.base_url,
                model: v.model,
                models: v.models,
                verified,
            });
        }
        self.settings.ai.custom_providers = rebuilt;
    }

    /// 判断该供应商是否要求非空 API Key
    pub fn ai_requires_api_key(r: &ProviderRef, base_url: &str) -> bool {
        match r {
            ProviderRef::Builtin(id) => id.requires_api_key(),
            ProviderRef::Custom(_) => !crate::models::settings::is_local_base_url(base_url),
        }
    }

    /// 根据测试状态与快照判定某供应商是否 verified
    fn ai_ref_verified(&self, r: &ProviderRef, v: &AiInputValues) -> bool {
        let pass = matches!(self.ai_test_statuses.get(r), Some(AiTestStatus::Pass));
        let matches_snapshot = self
            .ai_tested_snapshots
            .get(r)
            .map(|(k, b, m, f)| {
                *k == v.api_key && *b == v.base_url && *m == v.model && *f == v.format.label()
            })
            .unwrap_or(false);
        let has_credential =
            !Self::ai_requires_api_key(r, &v.base_url) || !v.api_key.trim().is_empty();
        has_credential && pass && matches_snapshot
    }

    /// 检查 AI 设置是否可以保存：每个填了 api_key 的供应商都必须通过测试且未被修改
    /// 返回 Err(供应商名称列表) 表示哪些供应商未通过校验
    pub fn validate_ai_for_save(&self, cx: &App) -> Result<(), Vec<String>> {
        let mut failing = Vec::new();
        let mut refs: Vec<ProviderRef> = AiProviderId::ALL
            .into_iter()
            .map(ProviderRef::Builtin)
            .collect();
        for c in &self.settings.ai.custom_providers {
            refs.push(ProviderRef::Custom(c.id.clone()));
        }
        for r in refs {
            let v = self.collect_ai_inputs(&r, cx);
            if v.api_key.trim().is_empty() {
                continue;
            }
            if !self.ai_ref_verified(&r, &v) {
                let name = match &r {
                    ProviderRef::Builtin(id) => id.label().to_string(),
                    ProviderRef::Custom(_) => {
                        if v.name.trim().is_empty() {
                            "(未命名)".to_string()
                        } else {
                            v.name.clone()
                        }
                    }
                };
                failing.push(name);
            }
        }
        if failing.is_empty() {
            Ok(())
        } else {
            Err(failing)
        }
    }

    /// 从当前表单实时读取 WebDAV 同步配置（未点击保存也可直接测试连通性与同步）
    pub fn collect_sync_settings(&self, cx: &App) -> crate::models::settings::SyncSettings {
        let mut cfg = self.settings.sync.clone();
        if let Some(input) = &self.webdav_url_input {
            cfg.webdav_url = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_username_input {
            cfg.webdav_username = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_password_input {
            cfg.webdav_password = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_path_input {
            cfg.webdav_path = input.read(cx).value().to_string();
        }
        if let Some(input) = &self.webdav_passphrase_input {
            cfg.sync_passphrase = input.read(cx).value().to_string();
        }
        cfg
    }

    /// 新增一个自定义供应商，返回其 ref
    pub fn add_custom_provider(&mut self) -> ProviderRef {
        let c = CustomProvider::new();
        let id = c.id.clone();
        self.settings.ai.custom_providers.push(c);
        ProviderRef::Custom(id)
    }

    /// 向指定供应商添加一个模型（自动去重）
    pub fn add_ai_model(&mut self, r: &ProviderRef, model: String) {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let default_list = match r {
            ProviderRef::Builtin(id) => self.settings.ai.get(*id).model_list(),
            ProviderRef::Custom(cid) => self
                .settings
                .ai
                .custom_get(cid)
                .map(|c| c.model_list())
                .unwrap_or_default(),
        };
        let entry = self.ai_inputs.entry(r.clone()).or_default();
        let list = entry.models.get_or_insert(default_list);
        if !list.iter().any(|m| m == &trimmed) {
            list.push(trimmed);
            self.mark_changed();
        }
    }

    /// 从指定供应商移除一个模型
    pub fn remove_ai_model(&mut self, r: &ProviderRef, model: &str) {
        if let Some(list) = self.ai_inputs.get_mut(r).and_then(|e| e.models.as_mut()) {
            let before = list.len();
            list.retain(|m| m != model);
            if list.len() != before {
                self.mark_changed();
            }
        }
    }

    /// 将指定模型提升为该供应商的默认模型（列表首项）
    pub fn set_default_ai_model(&mut self, r: &ProviderRef, model: &str) {
        if let Some(list) = self.ai_inputs.get_mut(r).and_then(|e| e.models.as_mut()) {
            if let Some(pos) = list.iter().position(|m| m == model) {
                if pos > 0 {
                    let item = list.remove(pos);
                    list.insert(0, item);
                    self.mark_changed();
                }
            }
        }
    }

    /// 获取某供应商当前已添加的模型列表
    pub fn current_ai_models(&self, r: &ProviderRef) -> Vec<String> {
        if let Some(list) = self.ai_inputs.get(r).and_then(|e| e.models.clone()) {
            return list;
        }
        match r {
            ProviderRef::Builtin(id) => self.settings.ai.get(*id).model_list(),
            ProviderRef::Custom(cid) => self
                .settings
                .ai
                .custom_get(cid)
                .map(|c| c.model_list())
                .unwrap_or_default(),
        }
    }

    /// 获取某供应商可供下拉选择添加的候选模型（合并远端拉取模型与内置预置模型，排除已添加项）
    pub fn candidate_ai_models(&self, r: &ProviderRef) -> Vec<String> {
        let selected = self.current_ai_models(r);
        let mut out: Vec<String> = Vec::new();
        let mut push_unique = |m: &str| {
            let m = m.trim();
            if !m.is_empty()
                && !selected.iter().any(|s| s == m)
                && !out.iter().any(|e| e == m)
            {
                out.push(m.to_string());
            }
        };
        if let Some(remote) = self.ai_remote_models.get(r) {
            for m in remote {
                push_unique(m);
            }
        }
        if let ProviderRef::Builtin(id) = r {
            for &m in id.default_models() {
                push_unique(m);
            }
        }
        out
    }

    /// 删除指定自定义供应商
    pub fn remove_custom_provider(&mut self, id: &str) {
        self.settings.ai.custom_providers.retain(|c| c.id != id);
        let r = ProviderRef::Custom(id.to_string());
        self.ai_inputs.remove(&r);
        self.ai_test_statuses.remove(&r);
        self.ai_tested_snapshots.remove(&r);
        self.ai_fetching_models.remove(&r);
        self.ai_remote_models.remove(&r);
        self.ai_fetch_notices.remove(&r);
    }
}

/// 立即切换界面的主题模式
pub fn apply_theme_mode(mode: &ThemeMode, window: &mut Window, cx: &mut App) {
    match mode {
        ThemeMode::Light => GpuiTheme::change(GpuiThemeMode::Light, Some(window), cx),
        ThemeMode::Dark => GpuiTheme::change(GpuiThemeMode::Dark, Some(window), cx),
        ThemeMode::System => GpuiTheme::sync_system_appearance(Some(window), cx),
    }
}

/// 期望的弹窗尺寸，窗口较小时会收缩
const DIALOG_SIZE: Size<Pixels> = size(px(860.), px(600.));
/// 左侧导航宽度
const NAV_WIDTH: Pixels = px(200.);
/// 内容区左右留白
const CONTENT_PADDING_X: Pixels = px(28.);

/// 渲染设置弹窗覆盖层
pub fn render_settings_dialog_overlay(
    state: Entity<SettingsDialogState>,
    viewport: Size<Pixels>,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let dialog_size = fit_to_viewport(DIALOG_SIZE, viewport);
    let focus_handle = state.read(cx).focus_handle.clone();

    let state_for_escape = state.clone();
    DialogOverlay::new()
        .focus(focus_handle.as_ref())
        .on_escape(move |window, cx| {
            state_for_escape.update(cx, |s, cx| s.cancel(window, cx));
        })
        .render(
            "settings-dialog-overlay",
            dialog_panel(&p)
                .flex_row()
                .w(dialog_size.width)
                .h(dialog_size.height)
                .bg(p.background)
                .child(render_left_nav(state.clone(), &p, cx))
                .child(render_right_content(state, &p, cx)),
            &p,
        )
}

/// 渲染左侧导航
fn render_left_nav(
    state: Entity<SettingsDialogState>,
    p: &crate::theme::Palette,
    cx: &App,
) -> impl IntoElement {
    let state_read = state.read(cx);
    let lang = state_read.settings.theme.language.clone();
    let current = state_read.current_section;

    dialog_sidebar(i18n::t(&lang, "settings.title"), NAV_WIDTH, p).child(
        dialog_nav_list("settings-nav").children(SettingsSection::ALL.into_iter().map(|section| {
            let state = state.clone();
            dialog_nav_item(
                SharedString::from(format!("settings-nav-{:?}", section)),
                section.icon(),
                i18n::t(&lang, section.label_key()),
                section == current,
                false,
                p,
            )
            .on_click(move |_, _, cx| {
                state.update(cx, |s, cx| {
                    s.select_section(section);
                    cx.notify();
                });
            })
        })),
    )
}

/// 渲染右侧：分区标题、内容和底栏
fn render_right_content(
    state: Entity<SettingsDialogState>,
    p: &crate::theme::Palette,
    cx: &App,
) -> impl IntoElement {
    let state_read = state.read(cx);
    let lang = state_read.settings.theme.language.clone();
    let section = state_read.current_section;
    let scroll_handle = state_read.scroll_handle.clone();
    let error = state_read
        .save_error
        .clone()
        .or_else(|| state_read.ai_save_error.clone());

    let state_for_close = state.clone();
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    div()
        .flex_1()
        // 允许收缩到可用宽度：否则较长的说明文字会按单行宽度撑开内容区，挤压左侧菜单并被裁切
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(dialog_pane_header(
            i18n::t(&lang, section.label_key()),
            i18n::t(&lang, "common.close"),
            move |window, cx| {
                state_for_close.update(cx, |s, cx| s.cancel(window, cx));
            },
            CONTENT_PADDING_X,
            p,
        ))
        .child(dialog_scroll_area(
            "settings-form-scroll",
            &scroll_handle,
            CONTENT_PADDING_X,
            render_section_content(state, cx),
        ))
        .child(
            dialog_pane_footer(error.map(SharedString::from), CONTENT_PADDING_X, p)
                .child(
                    text_button(
                        "settings-cancel-btn",
                        ButtonKind::Secondary,
                        i18n::t(&lang, "common.cancel"),
                        p,
                    )
                    .on_click(move |_, window, cx| {
                        state_for_cancel.update(cx, |s, cx| s.cancel(window, cx));
                    }),
                )
                .child(
                    text_button(
                        "settings-save-btn",
                        ButtonKind::Primary,
                        i18n::t(&lang, "common.save"),
                        p,
                    )
                    .on_click(move |_, _, cx| {
                        state_for_save.update(cx, |s, cx| s.try_save(cx));
                    }),
                ),
        )
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
        SettingsSection::Ai => render_ai_panel(state, cx).into_any_element(),
        SettingsSection::System => render_system_panel(state, cx).into_any_element(),
        SettingsSection::About => render_about_panel(state, cx).into_any_element(),
    }
}
