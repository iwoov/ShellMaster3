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
        }
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub accent_color: String,
    pub ui_font_family: String,
    pub ui_font_size: u32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
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
    // 编辑器
    pub use_builtin_editor: bool,
    pub external_editor_path: String,
    pub auto_save: bool,
    pub syntax_highlighting: bool,
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
            auto_save: false,
            syntax_highlighting: true,
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
