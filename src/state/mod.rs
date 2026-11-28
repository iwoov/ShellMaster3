// 全局 AppState 模块
// 按功能拆分为多个子模块

mod ai_chat;
mod core;
mod sftp_editor;
mod sftp_navigation;
mod sftp_transfer;
mod terminal;
mod ui_state;
pub mod updater;

pub use updater::UpdateState;

use crate::components::monitor::DetailDialogState;
use crate::components::sftp::{
    FileListView, NewFileDialogState, NewFolderDialogState, PathBarState, PropertiesDialogState,
};
use crate::models::monitor::MonitorState;
use crate::models::server::ServerData;
use crate::models::sftp::SftpState;
use crate::models::SnippetsConfig;
use crate::services::monitor::MonitorService;
use crate::services::sftp::{FileWatchEvent, FileWatcher, LineEnding, SftpService};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui_kit::component::input::{EditorState, InputState, TextareaState};
use gpui_kit::{Entity, FocusHandle, Subscription};

/// SFTP 删除二次确认对话框状态
#[derive(Clone)]
pub struct SftpDeleteConfirm {
    pub tab_id: String,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub focus_handle: FocusHandle,
}

/// 终端多行/大段粘贴二次确认弹窗状态
#[derive(Clone)]
pub struct PendingTerminalPaste {
    pub tab_id: String,
    pub text: String,
    pub line_count: usize,
    pub focus_handle: FocusHandle,
}

/// 超过 1 行或超过 4 KiB 的粘贴需要弹窗确认，避免误粘贴并执行多行脚本
pub fn needs_paste_confirmation(text: &str) -> bool {
    text.contains('\n') || text.contains('\r') || text.len() > 4096
}

/// 会话连接状态
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Error/Disconnected 将来用于错误处理
pub enum SessionStatus {
    Connecting,
    Connected,
    Error(String),
    Disconnected,
    /// 重连中，包含当前尝试次数和最大尝试次数
    Reconnecting {
        attempt: u32,
        max_attempts: u32,
    },
}

/// 中间主区当前展示模式（终端 / 内置 SFTP 文件编辑器）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CenterViewMode {
    #[default]
    Terminal,
    Editor,
}

/// 内置编辑器标签的缓冲区生命周期状态
#[derive(Clone)]
pub enum EditorBufferStatus {
    /// 正在通过 SFTP 拉取远程文件内容
    Loading,
    /// 文件已下载并校验完毕，等待渲染帧持有 Window 时实例化 `EditorState`
    PendingInit { content: String },
    /// 编辑器状态已就绪
    Ready { editor_state: Entity<EditorState> },
    /// 打开或加载失败
    Error(String),
}

/// 单个打开在内置编辑器中的远程文件标签
#[derive(Clone)]
pub struct SftpEditorTab {
    pub id: String,
    pub remote_path: String,
    pub file_name: String,
    /// 传给 `EditorState::language` 的语法高亮标识
    pub syntax_token: &'static str,
    /// 状态栏展示的语言名称
    pub language_name: &'static str,
    /// 原文件行尾符类型（保存写回时还原）
    pub line_ending: LineEnding,
    /// 原文件是否带有 UTF-8 BOM（保存写回时保留）
    pub has_bom: bool,
    /// 基准文本（统一为 LF），用于准确计算是否已修改
    pub original_content: Arc<String>,
    pub is_modified: bool,
    pub is_saving: bool,
    pub soft_wrap: bool,
    /// 远程重新加载完成后待同步到已就绪 `EditorState` 的文本
    pub pending_reload_content: Option<String>,
    pub status: EditorBufferStatus,
}

/// PTY 生命周期状态
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalPtyState {
    NotStarted,
    Opening,
    Ready,
    /// 远端 shell 已退出（连接仍在），可重新启动
    Exited,
    /// 打开终端失败（不含连接断开：断开交给会话状态处理），内容为给用户看的原因
    Failed(String),
}

impl TerminalPtyState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn can_start(&self) -> bool {
        matches!(self, Self::NotStarted)
    }

    /// 已结束（退出或失败），等待用户重新启动
    pub fn is_ended(&self) -> bool {
        matches!(self, Self::Exited | Self::Failed(_))
    }
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
    /// PTY 生命周期状态
    pub pty_state: TerminalPtyState,
    /// 上次发送给远端 PTY 的尺寸 (cols, rows)
    pub last_sent_pty_size: Option<(u32, u32)>,
    /// 应用通过 OSC 0/2 设置的标题（无则回退到 "Terminal N"）
    pub title: Option<String>,
}

/// 会话标签
#[derive(Clone)]
pub struct SessionTab {
    pub id: String,
    pub server_id: String,
    pub server_label: String,
    pub status: SessionStatus,
    /// 服务器连接数据（用于重连）
    pub server_data: Option<ServerData>,
    /// 最近一次断开/重连失败的原因（显示在“连接已断开”提示里），重新连上后清空
    pub disconnect_reason: Option<String>,
    /// 最近一次重连成功的时间：重连后很快又断开时不再自动重连，避免“连上 → 断开 → 重连”死循环
    pub reconnected_at: Option<std::time::Instant>,
    /// 中间主区当前展示模式（终端或内置编辑器）
    pub center_view: CenterViewMode,
    /// 多终端实例列表
    pub terminals: Vec<TerminalInstance>,
    /// 当前激活的终端 ID
    pub active_terminal_id: Option<String>,
    /// 终端计数器（用于生成标签）
    pub terminal_counter: u32,
    /// 内置编辑器打开的远程文件标签列表
    pub editor_tabs: Vec<SftpEditorTab>,
    /// 当前激活的内置编辑器文件标签 ID
    pub active_editor_tab_id: Option<String>,
    /// Monitor 监控状态
    pub monitor_state: MonitorState,
    /// SFTP 状态（懒加载）
    pub sftp_state: Option<SftpState>,
    /// 活动传输列表（上传/下载任务）
    pub active_transfers: Vec<crate::models::sftp::TransferItem>,
    /// 服务是否已启动（Monitor/SFTP，只在首次 PTY 创建时启动）
    pub services_started: bool,
    /// AI 对话状态（按 tab 内存保存，关闭 tab 即清）
    pub ai_chat: AiChatState,
    /// 当前会话运行中的 SSH 端口转发隧道列表（-L / -R / -D）
    pub port_forwards: Vec<crate::ssh::forwarding::PortForwardRule>,
}

/// 侧边栏面板类型
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SidebarPanel {
    #[default]
    AiChat,      // AI Agent
    Snippets,    // 快捷命令
    Transfer,    // 传输管理
    PortForward, // 端口转发
}

impl SidebarPanel {
    pub fn as_key(self) -> &'static str {
        match self {
            SidebarPanel::AiChat => "ai_chat",
            SidebarPanel::Snippets => "snippets",
            SidebarPanel::Transfer => "transfer",
            SidebarPanel::PortForward => "port_forward",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "ai_chat" => Some(SidebarPanel::AiChat),
            "snippets" => Some(SidebarPanel::Snippets),
            "transfer" => Some(SidebarPanel::Transfer),
            "port_forward" => Some(SidebarPanel::PortForward),
            _ => None,
        }
    }
}

/// Agent 工具调用在侧边栏里的执行状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCallStatus {
    /// 等待用户点击 允许 / 拒绝
    WaitingApproval,
    /// 正在执行
    Running,
    /// 执行成功
    Succeeded,
    /// 执行出错
    Failed,
    /// 用户拒绝执行
    Denied,
}

/// 助手回复中的有序内容块（支持思考 / 工具调用 / 文本交错渲染）
#[derive(Clone, Debug)]
pub enum AgentMessageBlock {
    Reasoning {
        text: String,
        collapsed: bool,
    },
    Text(String),
    ToolCall {
        call_id: String,
        tool_name: String,
        summary: String,
        risk: crate::services::agent::ToolRisk,
        status: ToolCallStatus,
        output: String,
        collapsed: bool,
    },
}

/// AI Agent 对话消息（按 tab 内存保存，关闭即清）
#[derive(Clone, Debug)]
pub struct AiChatMessage {
    pub role: crate::services::ai::ChatRole,
    pub content: String,
    /// 助手消息的结构化内容块；用户消息/错误消息为空，直接使用 `content`
    pub blocks: Vec<AgentMessageBlock>,
    pub error: bool,
}

impl AiChatMessage {
    /// 是否有任何可渲染内容（避免流式首 token 到达前出现空白占位气泡）
    pub fn has_visible_content(&self) -> bool {
        if !self.content.is_empty() || self.error {
            return true;
        }
        self.blocks.iter().any(|b| match b {
            AgentMessageBlock::Reasoning { text, .. } => !text.trim().is_empty(),
            AgentMessageBlock::Text(t) => !t.is_empty(),
            AgentMessageBlock::ToolCall { .. } => true,
        })
    }

    /// 构造传入下一轮对话历史的纯文本摘要（含工具执行结果摘要，方便多轮追问）
    pub fn history_text(&self) -> String {
        if self.blocks.is_empty() {
            return self.content.clone();
        }
        let mut parts: Vec<String> = Vec::new();
        for block in &self.blocks {
            match block {
                AgentMessageBlock::Text(t) if !t.trim().is_empty() => parts.push(t.clone()),
                AgentMessageBlock::ToolCall {
                    tool_name,
                    summary,
                    status,
                    ..
                } => {
                    parts.push(format!("[tool {tool_name} ({status:?}): {summary}]"));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            self.content.clone()
        } else {
            parts.join("\n")
        }
    }
}

/// 判定 AI 对话区“偏离底部”的距离阈值：超过此距离认为用户主动离开底部
pub const AI_CHAT_SCROLL_DETACH_THRESHOLD: gpui_kit::Pixels = gpui_kit::px(28.);
/// 判定 AI 对话区“已回到底部”的距离阈值：滚动回此范围内自动恢复贴底跟随
pub const AI_CHAT_SCROLL_REENGAGE_THRESHOLD: gpui_kit::Pixels = gpui_kit::px(4.);

/// 单个会话标签的 AI 对话状态
#[derive(Clone)]
pub struct AiChatState {
    pub messages: Vec<AiChatMessage>,
    pub pending: bool,
    /// 用户在 chat header 选择的供应商；None 表示使用 settings.ai.default_provider
    pub selected_provider: Option<crate::models::ProviderRef>,
    /// 用户在底部选择的模型；None 表示使用该供应商的默认模型
    pub selected_model: Option<String>,
    /// 消息列表滚动句柄（按 tab 独立，Clone 共享底层 Rc 状态）
    pub scroll_handle: gpui_kit::ScrollHandle,
    /// 是否处于智能贴底跟随状态（默认开启；用户上滑或拖离底部后暂停，滚回底部或发新消息后恢复）
    pub auto_scroll: bool,
}

impl Default for AiChatState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            pending: false,
            selected_provider: None,
            selected_model: None,
            scroll_handle: gpui_kit::ScrollHandle::new(),
            auto_scroll: true,
        }
    }
}

impl AiChatState {
    /// 当前滚动位置距离内容底部的像素差（<= 0 时夹紧为 0）
    pub fn distance_to_bottom(&self) -> gpui_kit::Pixels {
        let max_h = self.scroll_handle.max_offset().y;
        if max_h <= gpui_kit::px(0.) {
            gpui_kit::px(0.)
        } else {
            (max_h + self.scroll_handle.offset().y).max(gpui_kit::px(0.))
        }
    }

    /// 是否需要展示右下角「回到底部」悬浮按钮
    pub fn show_scroll_to_bottom_button(&self) -> bool {
        let max_h = self.scroll_handle.max_offset().y;
        max_h > gpui_kit::px(0.) && self.distance_to_bottom() > AI_CHAT_SCROLL_DETACH_THRESHOLD
    }

    /// 强制恢复贴底跟随并滚到底部（用户发消息、点击回到底部按钮时调用）
    pub fn pin_and_scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
        self.scroll_handle.scroll_to_bottom();
    }

    /// 若当前处于贴底跟随状态，则滚到底部；同时同步检测用户是否通过拖动滚动条离开了底部
    pub fn scroll_to_bottom_if_pinned(&mut self) {
        let max_h = self.scroll_handle.max_offset().y;
        let offset_y = self.scroll_handle.offset().y;
        self.auto_scroll =
            Self::next_auto_scroll_from_position(self.auto_scroll, max_h, offset_y);
        if self.auto_scroll {
            self.scroll_handle.scroll_to_bottom();
        }
    }

    /// 响应鼠标滚轮事件，更新贴底跟随状态
    pub fn on_scroll_wheel(&mut self, delta_y: gpui_kit::Pixels) {
        let max_h = self.scroll_handle.max_offset().y;
        let offset_y = self.scroll_handle.offset().y;
        self.auto_scroll =
            Self::next_auto_scroll_from_wheel(self.auto_scroll, max_h, offset_y, delta_y);
    }

    /// 清空对话时重置滚动位置并恢复贴底状态
    pub fn reset_scroll(&mut self) {
        self.auto_scroll = true;
        self.scroll_handle.set_offset(gpui_kit::Point::default());
    }

    pub(crate) fn next_auto_scroll_from_position(
        auto_scroll: bool,
        max_h: gpui_kit::Pixels,
        offset_y: gpui_kit::Pixels,
    ) -> bool {
        if max_h <= gpui_kit::px(0.) {
            return true;
        }
        let distance = (max_h + offset_y).max(gpui_kit::px(0.));
        if auto_scroll {
            distance <= AI_CHAT_SCROLL_DETACH_THRESHOLD
        } else {
            distance <= AI_CHAT_SCROLL_REENGAGE_THRESHOLD
        }
    }

    pub(crate) fn next_auto_scroll_from_wheel(
        auto_scroll: bool,
        max_h: gpui_kit::Pixels,
        offset_y: gpui_kit::Pixels,
        delta_y: gpui_kit::Pixels,
    ) -> bool {
        if max_h <= gpui_kit::px(0.) {
            return true;
        }
        if delta_y > gpui_kit::px(0.) {
            return false;
        }
        if delta_y < gpui_kit::px(0.) {
            let projected_y = (offset_y + delta_y).max(-max_h);
            let effective_y = offset_y.min(projected_y);
            let distance = (max_h + effective_y).max(gpui_kit::px(0.));
            if distance <= AI_CHAT_SCROLL_REENGAGE_THRESHOLD {
                return true;
            }
        }
        auto_scroll
    }
}

/// 全局会话状态
pub struct SessionState {
    pub tabs: Vec<SessionTab>,
    pub active_tab_id: Option<String>,
    /// 是否显示主页视图（即使有会话也可以切换到主页）
    pub show_home: bool,
    /// 右侧 Sidebar 是否折叠
    pub sidebar_collapsed: bool,
    /// 会话视图左列（监控、SFTP 目录树）的宽度，所有标签共用；实际宽度还要按窗口宽度调整，见 `pages::session::fit_columns`
    pub left_column_width: gpui_kit::Pixels,
    /// 右侧侧栏的宽度，所有标签共用
    pub sidebar_width: gpui_kit::Pixels,
    /// 当前激活的侧边栏面板
    pub active_sidebar_panel: SidebarPanel,
    /// 快捷命令树展开的组 ID 集合
    pub snippets_expanded: HashSet<String>,
    /// 快捷命令配置缓存
    pub snippets_config: Option<SnippetsConfig>,
    /// 终端命令输入状态
    pub command_input: Option<Entity<TextareaState>>,
    /// 终端搜索输入框状态
    pub search_input: Option<Entity<InputState>>,
    /// 搜索栏是否展开（针对当前激活终端）
    pub search_open: bool,
    /// 搜索输入框事件订阅
    pub search_subscription: Option<Subscription>,
    /// 终端焦点句柄（用于键盘事件处理）
    pub terminal_focus_handle: Option<FocusHandle>,
    /// 终端焦点监听订阅（用于 xterm focus in/out 上报）
    pub terminal_focus_subscriptions: Vec<Subscription>,
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
    /// 内置编辑器变更监听订阅（按 editor_tab_id 存储）
    pub sftp_editor_subscriptions: HashMap<String, Subscription>,
    /// 待确认关闭的未保存编辑器标签：(session_tab_id, editor_tab_id)
    pub pending_close_editor_tab: Option<(String, String)>,
    /// 待确认重新加载的未保存编辑器标签：(session_tab_id, editor_tab_id)
    pub pending_reload_editor_tab: Option<(String, String)>,
    /// 待确认关闭的含有未保存文件的 SSH 会话标签 ID
    pub pending_close_session_tab: Option<String>,
    /// SFTP 删除二次确认弹窗状态
    pub sftp_delete_confirm: Option<SftpDeleteConfirm>,
    /// 终端多行/大段粘贴二次确认弹窗状态
    pub pending_terminal_paste: Option<PendingTerminalPaste>,
    /// SFTP 新建文件夹对话框状态
    pub sftp_new_folder_dialog: Option<Entity<NewFolderDialogState>>,
    /// SFTP 新建文件对话框状态
    pub sftp_new_file_dialog: Option<Entity<NewFileDialogState>>,
    /// SFTP 属性对话框状态
    pub sftp_properties_dialog: Option<Entity<PropertiesDialogState>>,
    /// 外置编辑器文件监控器
    pub file_watcher: Option<Arc<Mutex<FileWatcher>>>,
    /// 文件监控事件接收器
    pub file_watch_receiver: Option<std::sync::mpsc::Receiver<FileWatchEvent>>,
    /// AI Agent 对话输入框（按 tab_id 存储）
    pub ai_chat_inputs: HashMap<String, Entity<TextareaState>>,
    /// AI Agent 运行中的取消令牌（按 tab_id 存储）
    pub ai_cancel_tokens: HashMap<String, tokio_util::sync::CancellationToken>,
    /// AI Agent 等待用户审批的回传通道（按 call_id 存储）
    pub ai_pending_approvals: HashMap<
        String,
        tokio::sync::oneshot::Sender<crate::services::agent::ApprovalDecision>,
    >,
    /// AI Agent 本次会话允许自动放行的工具名集合（按 tab_id 存储，新建对话/关闭 tab 清空）
    pub ai_session_allowed_tools: HashMap<String, Arc<Mutex<HashSet<String>>>>,
    /// 刚复制过的条目（例如监控里的地址、快捷命令），图标短暂显示为对勾
    pub copied_key: Option<String>,
    /// 每次复制递增，旧的定时器到期时不会清掉新的复制反馈
    pub copy_feedback_token: u64,
    /// 端口转发侧栏表单状态
    pub forward_kind: crate::ssh::forwarding::ForwardingKind,
    pub forward_bind_host_input: Option<Entity<InputState>>,
    pub forward_bind_port_input: Option<Entity<InputState>>,
    pub forward_target_host_input: Option<Entity<InputState>>,
    pub forward_target_port_input: Option<Entity<InputState>>,
    pub forward_error: Option<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: None,
            show_home: true,
            sidebar_collapsed: true,
            left_column_width: crate::pages::session::columns::LEFT_COLUMN_DEFAULT,
            sidebar_width: crate::pages::session::columns::SIDEBAR_DEFAULT,
            active_sidebar_panel: SidebarPanel::AiChat,
            snippets_expanded: HashSet::new(),
            snippets_config: None,
            command_input: None,
            search_input: None,
            search_open: false,
            search_subscription: None,
            terminal_focus_handle: None,
            terminal_focus_subscriptions: Vec::new(),
            monitor_detail_dialog: None,
            monitor_services: Arc::new(Mutex::new(HashMap::new())),
            sftp_services: Arc::new(Mutex::new(HashMap::new())),
            sftp_file_list_views: HashMap::new(),
            sftp_path_bar_states: HashMap::new(),
            sftp_editor_subscriptions: HashMap::new(),
            pending_close_editor_tab: None,
            pending_reload_editor_tab: None,
            pending_close_session_tab: None,
            sftp_delete_confirm: None,
            pending_terminal_paste: None,
            sftp_new_folder_dialog: None,
            sftp_new_file_dialog: None,
            sftp_properties_dialog: None,
            file_watcher: None,
            file_watch_receiver: None,
            ai_chat_inputs: HashMap::new(),
            ai_cancel_tokens: HashMap::new(),
            ai_pending_approvals: HashMap::new(),
            ai_session_allowed_tools: HashMap::new(),
            copied_key: None,
            copy_feedback_token: 0,
            forward_kind: crate::ssh::forwarding::ForwardingKind::Local,
            forward_bind_host_input: None,
            forward_bind_port_input: None,
            forward_target_host_input: None,
            forward_target_port_input: None,
            forward_error: None,
        }
    }
}

impl SessionState {
    /// 按已持久化的本地界面布局状态初始化会话状态
    pub fn from_ui_state(ui_state: &crate::services::storage::UiState) -> Self {
        let active_sidebar_panel = ui_state
            .session_sidebar_panel
            .as_deref()
            .and_then(SidebarPanel::from_key)
            .unwrap_or_default();
        Self {
            sidebar_collapsed: ui_state.session_sidebar_collapsed,
            active_sidebar_panel,
            ..Self::default()
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

/// 计算路径层级列表（如 /home/alice -> ["/", "/home", "/home/alice"]）
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
            let full_path = crate::models::sftp::join_path(base_path, &name);
            let attrs = entry.metadata();
            let file_type = if attrs.is_dir() {
                crate::models::sftp::FileType::Directory
            } else if attrs.is_symlink() {
                crate::models::sftp::FileType::Symlink
            } else if attrs.is_regular() {
                crate::models::sftp::FileType::File
            } else {
                crate::models::sftp::FileType::Other
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
