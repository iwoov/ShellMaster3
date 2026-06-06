// Settings 配置数据结构

use serde::{Deserialize, Serialize};

// ======================== 主配置结构 ========================

/// 应用设置（持久化用）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: ThemeSettings,
    pub terminal: TerminalSettings,
    pub sftp: SftpSettings,
    pub monitor: MonitorSettings,
    pub connection: ConnectionSettings,
    pub sync: SyncSettings,
    pub system: SystemSettings,
    #[serde(default)]
    pub ai: AiSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeSettings::default(),
            terminal: TerminalSettings::default(),
            sftp: SftpSettings::default(),
            monitor: MonitorSettings::default(),
            connection: ConnectionSettings::default(),
            sync: SyncSettings::default(),
            system: SystemSettings::default(),
            ai: AiSettings::default(),
        }
    }
}

// ======================== AI 设置 ========================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AiProviderId {
    OpenAi,
    Gemini,
    Claude,
    DeepSeek,
}

impl AiProviderId {
    pub const ALL: [AiProviderId; 4] = [
        AiProviderId::OpenAi,
        AiProviderId::Gemini,
        AiProviderId::Claude,
        AiProviderId::DeepSeek,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AiProviderId::OpenAi => "OpenAI",
            AiProviderId::Gemini => "Gemini",
            AiProviderId::Claude => "Claude",
            AiProviderId::DeepSeek => "DeepSeek",
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            AiProviderId::OpenAi => "openai",
            AiProviderId::Gemini => "gemini",
            AiProviderId::Claude => "claude",
            AiProviderId::DeepSeek => "deepseek",
        }
    }

    pub fn default_base_url(&self) -> &'static str {
        match self {
            AiProviderId::OpenAi => "https://api.openai.com/v1",
            AiProviderId::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            AiProviderId::Claude => "https://api.anthropic.com/v1",
            AiProviderId::DeepSeek => "https://api.deepseek.com/v1",
        }
    }

    pub fn default_model(&self) -> &'static str {
        self.default_models()[0]
    }

    /// 各供应商预置的常见模型列表（首项作为默认模型）
    pub fn default_models(&self) -> &'static [&'static str] {
        match self {
            AiProviderId::OpenAi => &["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini"],
            AiProviderId::Gemini => &["gemini-1.5-flash", "gemini-1.5-pro", "gemini-2.0-flash"],
            AiProviderId::Claude => &[
                "claude-3-5-haiku-latest",
                "claude-3-5-sonnet-latest",
                "claude-3-7-sonnet-latest",
            ],
            AiProviderId::DeepSeek => &["deepseek-chat", "deepseek-reasoner"],
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            AiProviderId::OpenAi => crate::constants::icons::AI_OPENAI,
            AiProviderId::Gemini => crate::constants::icons::AI_GEMINI,
            AiProviderId::Claude => crate::constants::icons::AI_CLAUDE,
            AiProviderId::DeepSeek => crate::constants::icons::AI_DEEPSEEK,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiProviderConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    /// 当前/默认使用的模型（= models 列表第一项）
    #[serde(default)]
    pub model: String,
    /// 该供应商可选的模型列表（侧边栏下拉用）
    #[serde(default)]
    pub models: Vec<String>,
    /// 是否已通过连通性测试，仅通过后才允许保存
    #[serde(default)]
    pub verified: bool,
}

impl AiProviderConfig {
    pub fn for_provider(id: AiProviderId) -> Self {
        let models: Vec<String> = id.default_models().iter().map(|s| s.to_string()).collect();
        Self {
            api_key: String::new(),
            base_url: id.default_base_url().to_string(),
            model: id.default_model().to_string(),
            models,
            verified: false,
        }
    }

    /// 返回可选模型列表：models 非空则用之，否则回退到单个 model
    pub fn model_list(&self) -> Vec<String> {
        if !self.models.is_empty() {
            self.models.clone()
        } else if !self.model.is_empty() {
            vec![self.model.clone()]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSettings {
    /// 默认对话使用的供应商
    pub default_provider: AiProviderId,
    /// 各供应商配置（按 provider key 存储）
    pub providers: std::collections::HashMap<String, AiProviderConfig>,
    /// 全局系统提示词（每次对话注入到 history 首部）
    #[serde(default = "AiSettings::default_system_prompt")]
    pub system_prompt: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        let mut providers = std::collections::HashMap::new();
        for id in AiProviderId::ALL {
            providers.insert(id.key().to_string(), AiProviderConfig::for_provider(id));
        }
        Self {
            default_provider: AiProviderId::OpenAi,
            providers,
            system_prompt: Self::default_system_prompt(),
        }
    }
}

impl AiSettings {
    pub fn default_system_prompt() -> String {
        "你是一名专业的 Linux 终端助手，专注于帮助用户解决 Shell / SSH / 服务器运维相关的问题。请遵循以下原则：\n\
1. 回复尽量精炼，先给出结论或可执行的命令，再附简要解释；\n\
2. 所有命令使用 Markdown 代码块包裹，并标注语言（如 ```bash），多步骤命令分块给出；\n\
3. 若命令具有破坏性（rm/mkfs/dd/chmod 777 等），必须在命令前用一句话明确警告；\n\
4. 不确定的环境差异（发行版/内核版本/权限）请先询问或在回答里列出假设；\n\
5. 优先给出 POSIX 通用方案，必要时再补充 GNU/BSD 差异；\n\
6. 中文提问用中文回答，英文提问用英文回答。"
            .to_string()
    }

    pub fn get(&self, id: AiProviderId) -> AiProviderConfig {
        self.providers
            .get(id.key())
            .cloned()
            .unwrap_or_else(|| AiProviderConfig::for_provider(id))
    }

    pub fn set(&mut self, id: AiProviderId, cfg: AiProviderConfig) {
        self.providers.insert(id.key().to_string(), cfg);
    }

    /// 返回已通过连通性测试的供应商列表
    pub fn verified_providers(&self) -> Vec<AiProviderId> {
        AiProviderId::ALL
            .into_iter()
            .filter(|id| {
                self.providers
                    .get(id.key())
                    .map(|c| c.verified && !c.api_key.is_empty())
                    .unwrap_or(false)
            })
            .collect()
    }
}

// ======================== 主题设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum Language {
    #[default]
    Chinese,
    English,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::Chinese => "简体中文",
            Language::English => "English",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub language: Language,
    pub accent_color: String,
    pub ui_font_family: String,
    pub ui_font_size: u32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            language: Language::Chinese,
            accent_color: "#3b82f6".to_string(), // Blue
            ui_font_family: "system-ui".to_string(),
            ui_font_size: 14,
        }
    }
}

// ======================== 终端设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum BellStyle {
    #[default]
    None,
    Visual,
    Sound,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalSettings {
    // 字体
    pub font_family: String,
    pub font_size: u32,
    pub line_height: f32,
    pub font_weight: FontWeight,
    pub ligatures: bool,
    // 配色
    pub color_scheme: String,
    pub foreground_color: String,
    pub background_color: String,
    pub cursor_color: String,
    pub selection_color: String,
    // 显示
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub background_opacity: u32,
    pub scrollback_lines: u32,
    // 行为
    pub copy_on_select: bool,
    pub right_click_paste: bool,
    pub trim_trailing_whitespace: bool,
    pub scroll_on_output: bool,
    pub bell_style: BellStyle,
    pub word_separators: String,
    // Shell
    pub default_shell: String,
    pub shell_args: String,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            line_height: 1.2,
            font_weight: FontWeight::Normal,
            ligatures: true,
            color_scheme: "One Dark".to_string(),
            foreground_color: "#abb2bf".to_string(),
            background_color: "#282c34".to_string(),
            cursor_color: "#528bff".to_string(),
            selection_color: "#3e4451".to_string(),
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            background_opacity: 100,
            scrollback_lines: 10000,
            copy_on_select: false,
            right_click_paste: true,
            trim_trailing_whitespace: true,
            scroll_on_output: true,
            bell_style: BellStyle::None,
            word_separators: " <>()\"':;,│".to_string(),
            default_shell: String::new(), // Use system default
            shell_args: String::new(),
        }
    }
}

// ======================== SFTP 设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SftpViewMode {
    #[default]
    List,
    Icons,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SftpSortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ConflictAction {
    #[default]
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SftpSettings {
    // 文件显示
    pub default_view_mode: SftpViewMode,
    pub show_hidden_files: bool,
    pub show_extensions: bool,
    pub sort_by: SftpSortBy,
    pub sort_ascending: bool,
    pub folders_first: bool,
    // 传输
    pub concurrent_transfers: u32,
    pub conflict_action: ConflictAction,
    pub preserve_timestamps: bool,
    pub speed_limit_kb: u32,
    pub resume_transfers: bool,
    pub open_folder_after_download: bool,
    // 路径
    pub local_default_path: String,
    pub remote_default_path: String,
    pub remember_last_path: bool,
    // 编辑器 - 基本设置
    pub use_builtin_editor: bool,
    pub external_editor_path: String,
    pub max_edit_file_size_kb: u32, // 最大可编辑文件大小 (KB)
    // 编辑器 - 内置编辑器外观
    pub editor_font_family: String,
    pub editor_font_size: u32,
    pub editor_line_height: f32,
    pub editor_gutter_width: u32,
    pub editor_gutter_padding: u32,
}

impl Default for SftpSettings {
    fn default() -> Self {
        Self {
            default_view_mode: SftpViewMode::List,
            show_hidden_files: false,
            show_extensions: true,
            sort_by: SftpSortBy::Name,
            sort_ascending: true,
            folders_first: true,
            concurrent_transfers: 3,
            conflict_action: ConflictAction::Ask,
            preserve_timestamps: true,
            speed_limit_kb: 0,
            resume_transfers: true,
            open_folder_after_download: false,
            local_default_path: String::new(),
            remote_default_path: String::new(),
            remember_last_path: true,
            use_builtin_editor: true,
            external_editor_path: String::new(),
            max_edit_file_size_kb: 5120, // 5MB
            editor_font_family: "JetBrains Mono".to_string(),
            editor_font_size: 14,
            editor_line_height: 1.4,
            editor_gutter_width: 48,
            editor_gutter_padding: 8,
        }
    }
}

// ======================== 监控设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum MonitorInterval {
    Sec1,
    #[default]
    Sec2,
    Sec5,
    Sec10,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ChartStyle {
    #[default]
    Line,
    Area,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitorSettings {
    // 数据采集
    pub refresh_interval: MonitorInterval,
    pub history_retention_minutes: u32,
    pub auto_deploy_agent: bool,
    // 显示
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disk: bool,
    pub show_network: bool,
    pub show_processes: bool,
    pub chart_style: ChartStyle,
    pub show_grid: bool,
    pub temperature_unit: TemperatureUnit,
    // 告警
    pub cpu_alert_threshold: u32,
    pub memory_alert_threshold: u32,
    pub disk_alert_threshold: u32,
    pub alert_notification: bool,
    pub alert_sound: bool,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            refresh_interval: MonitorInterval::Sec2,
            history_retention_minutes: 5,
            auto_deploy_agent: true,
            show_cpu: true,
            show_memory: true,
            show_disk: true,
            show_network: true,
            show_processes: true,
            chart_style: ChartStyle::Line,
            show_grid: true,
            temperature_unit: TemperatureUnit::Celsius,
            cpu_alert_threshold: 90,
            memory_alert_threshold: 85,
            disk_alert_threshold: 90,
            alert_notification: true,
            alert_sound: false,
        }
    }
}

// ======================== 连接设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum GlobalProxyType {
    #[default]
    None,
    Http,
    Socks4,
    Socks5,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionSettings {
    // SSH
    pub default_port: u16,
    pub connection_timeout_secs: u32,
    pub keepalive_interval_secs: u32,
    pub compression: bool,
    pub strict_host_key_checking: bool,
    // 自动重连
    pub auto_reconnect: bool,
    pub reconnect_attempts: u32,
    pub reconnect_interval_secs: u32,
    pub restore_session: bool,
    // 全局代理
    pub global_proxy_type: GlobalProxyType,
    pub global_proxy_host: String,
    pub global_proxy_port: u16,
    pub global_proxy_username: String,
    pub global_proxy_password: String,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            default_port: 22,
            connection_timeout_secs: 30,
            keepalive_interval_secs: 60,
            compression: false,
            strict_host_key_checking: false,
            auto_reconnect: true,
            reconnect_attempts: 3,
            reconnect_interval_secs: 5,
            restore_session: false,
            global_proxy_type: GlobalProxyType::None,
            global_proxy_host: String::new(),
            global_proxy_port: 0,
            global_proxy_username: String::new(),
            global_proxy_password: String::new(),
        }
    }
}

// ======================== 数据同步设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SyncMethod {
    #[default]
    None,
    WebDAV,
    ICloud,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SyncInterval {
    #[default]
    Manual,
    OnStartup,
    Hourly,
    Daily,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ConflictStrategy {
    #[default]
    Ask,
    LocalFirst,
    RemoteFirst,
    Merge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncSettings {
    pub enabled: bool,
    pub method: SyncMethod,
    pub auto_sync: bool,
    pub sync_interval: SyncInterval,
    // WebDAV
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_password: String,
    pub webdav_path: String,
    // 同步内容
    pub sync_servers: bool,
    pub sync_groups: bool,
    pub sync_settings: bool,
    pub sync_keybindings: bool,
    pub sync_keys: bool,
    // 冲突处理
    pub conflict_strategy: ConflictStrategy,
    pub backup_before_sync: bool,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            method: SyncMethod::None,
            auto_sync: false,
            sync_interval: SyncInterval::Manual,
            webdav_url: String::new(),
            webdav_username: String::new(),
            webdav_password: String::new(),
            webdav_path: "/shellmaster".to_string(),
            sync_servers: true,
            sync_groups: true,
            sync_settings: true,
            sync_keybindings: true,
            sync_keys: false,
            conflict_strategy: ConflictStrategy::Ask,
            backup_before_sync: true,
        }
    }
}

// ======================== 系统设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum AutoLockTime {
    #[default]
    Never,
    Min5,
    Min15,
    Hour1,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum HistoryRetention {
    #[default]
    Forever,
    Days7,
    Days30,
    OnExit,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemSettings {
    // 启动
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub restore_sessions: bool,
    pub check_updates: bool,
    // 窗口
    pub close_to_tray: bool,
    pub show_tray_icon: bool,
    pub single_instance: bool,
    pub save_window_position: bool,
    // 通知
    pub notify_on_connect: bool,
    pub notify_on_disconnect: bool,
    pub notify_on_transfer: bool,
    pub do_not_disturb: bool,
    // 隐私
    pub master_password_enabled: bool,
    pub auto_lock: AutoLockTime,
    pub history_retention: HistoryRetention,
    pub clear_clipboard_on_exit: bool,
    // 日志
    pub logging_enabled: bool,
    pub log_level: LogLevel,
    pub log_retention_days: u32,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized: false,
            restore_sessions: false,
            check_updates: true,
            close_to_tray: false,
            show_tray_icon: true,
            single_instance: true,
            save_window_position: true,
            notify_on_connect: false,
            notify_on_disconnect: true,
            notify_on_transfer: true,
            do_not_disturb: false,
            master_password_enabled: false,
            auto_lock: AutoLockTime::Never,
            history_retention: HistoryRetention::Forever,
            clear_clipboard_on_exit: false,
            logging_enabled: true,
            log_level: LogLevel::Info,
            log_retention_days: 7,
        }
    }
}
