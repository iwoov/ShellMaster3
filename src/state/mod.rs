// 全局 AppState 模块
// 按功能拆分为多个子模块

mod core;
mod sftp_navigation;
mod sftp_transfer;
mod terminal;
mod ui_state;

use crate::components::monitor::DetailDialogState;
use crate::components::sftp::{
    FileListView, NewFileDialogState, NewFolderDialogState, PathBarState,
};
use crate::models::monitor::MonitorState;
use crate::models::sftp::SftpState;
use crate::models::SnippetsConfig;
use crate::services::monitor::MonitorService;
use crate::services::sftp::SftpService;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::{Entity, FocusHandle};
use gpui_component::input::InputState;

/// 会话连接状态
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Error/Disconnected 将来用于错误处理
pub enum SessionStatus {
    Connecting,
    Connected,
    Error(String),
    Disconnected,
}

/// 单个终端实例
#[derive(Clone)]
pub struct TerminalInstance {
    pub id: String,
    /// 终端编号（用于生成翻译后的标签，如 "Terminal 1"）
    pub index: u32,
    /// 终端状态
    pub terminal: Option<Entity<crate::terminal::TerminalState>>,
    /// PTY 通道
    pub pty_channel: Option<std::sync::Arc<crate::ssh::session::TerminalChannel>>,
    /// PTY 是否已初始化
    pub pty_initialized: bool,
    /// 上次发送给远端 PTY 的尺寸 (cols, rows)
    pub last_sent_pty_size: Option<(u32, u32)>,
    /// PTY 错误信息
    pub pty_error: Option<String>,
}

/// 会话标签
#[derive(Clone)]
pub struct SessionTab {
    pub id: String,
    pub server_id: String,
    pub server_label: String,
    pub status: SessionStatus,
    /// 多终端实例列表
    pub terminals: Vec<TerminalInstance>,
    /// 当前激活的终端 ID
    pub active_terminal_id: Option<String>,
    /// 终端计数器（用于生成标签）
    pub terminal_counter: u32,
    /// Monitor 监控状态
    pub monitor_state: MonitorState,
    /// SFTP 状态（懒加载）
    pub sftp_state: Option<SftpState>,
    /// 活动传输列表（上传/下载任务）
    pub active_transfers: Vec<crate::models::sftp::TransferItem>,
}

/// 侧边栏面板类型
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SidebarPanel {
    #[default]
    Snippets, // 快捷命令
    Transfer, // 传输管理
}

/// 全局会话状态
pub struct SessionState {
    pub tabs: Vec<SessionTab>,
    pub active_tab_id: Option<String>,
    /// 是否显示主页视图（即使有会话也可以切换到主页）
    pub show_home: bool,
    /// 右侧 Sidebar 是否折叠
    pub sidebar_collapsed: bool,
    /// 当前激活的侧边栏面板
    pub active_sidebar_panel: SidebarPanel,
    /// 快捷命令树展开的组 ID 集合
    pub snippets_expanded: HashSet<String>,
    /// 快捷命令配置缓存
    pub snippets_config: Option<SnippetsConfig>,
    /// 终端命令输入状态
    pub command_input: Option<Entity<InputState>>,
    /// 终端焦点句柄（用于键盘事件处理）
    pub terminal_focus_handle: Option<FocusHandle>,
    /// Monitor 详情弹窗状态
    pub monitor_detail_dialog: Option<Entity<DetailDialogState>>,
    /// Monitor 服务实例（按 tab_id 存储）
    pub monitor_services: Arc<Mutex<HashMap<String, MonitorService>>>,
    /// SFTP 服务实例（按 tab_id 存储）
    pub sftp_services: Arc<Mutex<HashMap<String, SftpService>>>,
    /// SFTP 文件列表视图（按 tab_id 存储）
    pub sftp_file_list_views: HashMap<String, Entity<FileListView>>,
    /// SFTP 路径栏状态（按 tab_id 存储）
    pub sftp_path_bar_states: HashMap<String, Entity<PathBarState>>,
    /// SFTP 新建文件夹对话框状态
    pub sftp_new_folder_dialog: Option<Entity<NewFolderDialogState>>,
    /// SFTP 新建文件对话框状态
    pub sftp_new_file_dialog: Option<Entity<NewFileDialogState>>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: None,
            show_home: true,
            sidebar_collapsed: false,
            active_sidebar_panel: SidebarPanel::Snippets,
            snippets_expanded: HashSet::new(),
            snippets_config: None,
            command_input: None,
            terminal_focus_handle: None,
            monitor_detail_dialog: None,
            monitor_services: Arc::new(Mutex::new(HashMap::new())),
            sftp_services: Arc::new(Mutex::new(HashMap::new())),
            sftp_file_list_views: HashMap::new(),
            sftp_path_bar_states: HashMap::new(),
            sftp_new_folder_dialog: None,
            sftp_new_file_dialog: None,
        }
    }
}

/// SFTP 初始化结果
pub(crate) enum SftpInitResult {
    /// 1. 主目录路径就绪（工具栏可渲染）
    HomeReady {
        home_dir: String,
    },
    /// 2. 当前目录内容就绪（文件列表可渲染）
    CurrentDirReady {
        path: String,
        entries: Vec<crate::models::sftp::FileEntry>,
    },
    /// 3. 父级目录内容就绪（文件夹树可渲染）
    ParentDirsReady {
        dir_caches: Vec<(String, Vec<crate::models::sftp::FileEntry>)>,
    },
    /// 用户/组映射就绪（非关键路径，后台处理）
    UserGroupReady {
        passwd_content: Option<String>,
        group_content: Option<String>,
    },
    Error(String),
}

/// 计算路径层级列表（如 /home/wuyun -> ["/", "/home", "/home/wuyun"]）
pub(crate) fn get_path_hierarchy(path: &str) -> Vec<String> {
    let mut hierarchy = Vec::new();
    hierarchy.push("/".to_string());

    if path == "/" {
        return hierarchy;
    }

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current = String::new();
    for part in parts {
        current.push('/');
        current.push_str(part);
        hierarchy.push(current.clone());
    }

    hierarchy
}

/// 将 russh-sftp 的目录条目转换为 FileEntry
pub(crate) fn convert_sftp_entries(
    base_path: &str,
    entries: Vec<russh_sftp::client::fs::DirEntry>,
) -> Vec<crate::models::sftp::FileEntry> {
    entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            if name == "." || name == ".." {
                return None;
            }
            let full_path = if base_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", base_path.trim_end_matches('/'), name)
            };
            let attrs = entry.metadata();
            let file_type = if attrs.is_dir() {
                crate::models::sftp::FileType::Directory
            } else if attrs.is_symlink() {
                crate::models::sftp::FileType::Symlink
            } else {
                crate::models::sftp::FileType::File
            };
            let mut file_entry =
                crate::models::sftp::FileEntry::new(name.to_string(), full_path, file_type);
            file_entry.size = attrs.size.unwrap_or(0);
            file_entry.permissions = attrs.permissions.map(|p| p as u32).unwrap_or(0);
            file_entry.uid = attrs.uid;
            file_entry.gid = attrs.gid;
            if let Some(mtime) = attrs.mtime {
                file_entry.modified =
                    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64));
            }
            Some(file_entry)
        })
        .collect()
}
