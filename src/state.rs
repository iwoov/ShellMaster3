// 全局 AppState

use crate::models::SnippetsConfig;
use std::collections::HashSet;

use gpui::prelude::*;
use gpui::Entity;
use gpui_component::input::InputState;
use tracing::{debug, error, info};

/// 会话连接状态
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Error/Disconnected 将来用于错误处理
pub enum SessionStatus {
    Connecting,
    Connected,
    Error(String),
    Disconnected,
}

/// 会话标签
#[derive(Clone)]
pub struct SessionTab {
    pub id: String,
    pub server_id: String,
    pub server_label: String,
    pub status: SessionStatus,
    /// 终端状态
    pub terminal: Option<Entity<crate::terminal::TerminalState>>,
    /// PTY 通道
    pub pty_channel: Option<std::sync::Arc<crate::ssh::session::TerminalChannel>>,
    /// PTY 是否已初始化
    pub pty_initialized: bool,
    /// PTY 错误信息
    pub pty_error: Option<String>,
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
        }
    }
}

impl SessionState {
    /// 添加新的会话标签（插入到最前面）
    pub fn add_tab(&mut self, server_id: String, server_label: String) -> String {
        let tab_id = uuid::Uuid::new_v4().to_string();
        let tab = SessionTab {
            id: tab_id.clone(),
            server_id,
            server_label,
            status: SessionStatus::Connecting,
            terminal: None,
            pty_channel: None,
            pty_initialized: false,
            pty_error: None,
        };
        // 新标签插入到最前面
        self.tabs.insert(0, tab);
        self.active_tab_id = Some(tab_id.clone());
        // 切换到会话视图
        self.show_home = false;
        // 确保默认面板（快捷命令）的数据已加载
        self.load_snippets_config();
        tab_id
    }

    /// 关闭标签
    pub fn close_tab(&mut self, tab_id: &str) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.remove(pos);
            // 如果关闭的是当前活动标签，切换到下一个
            if self.active_tab_id.as_deref() == Some(tab_id) {
                self.active_tab_id = self.tabs.first().map(|t| t.id.clone());
            }
        }
    }

    /// 激活指定标签
    pub fn activate_tab(&mut self, tab_id: &str) {
        if self.tabs.iter().any(|t| t.id == tab_id) {
            self.active_tab_id = Some(tab_id.to_string());
        }
    }

    /// 更新标签状态
    pub fn update_tab_status(&mut self, tab_id: &str, status: SessionStatus) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = status;
        }
    }

    /// 获取当前活动标签
    pub fn active_tab(&self) -> Option<&SessionTab> {
        self.active_tab_id
            .as_ref()
            .and_then(|id| self.tabs.iter().find(|t| &t.id == id))
    }

    /// 检查是否有任何会话标签
    pub fn has_sessions(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// 切换 Sidebar 折叠状态
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    /// 设置当前激活的侧边栏面板
    pub fn set_sidebar_panel(&mut self, panel: SidebarPanel) {
        self.active_sidebar_panel = panel;
        // 如果 sidebar 折叠了，自动展开
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
    }

    /// 切换快捷命令组的展开状态
    pub fn toggle_snippets_group(&mut self, group_id: &str) {
        if self.snippets_expanded.contains(group_id) {
            self.snippets_expanded.remove(group_id);
        } else {
            self.snippets_expanded.insert(group_id.to_string());
        }
    }

    /// 加载快捷命令配置（如果尚未加载）
    pub fn load_snippets_config(&mut self) {
        if self.snippets_config.is_none() {
            self.snippets_config = crate::services::storage::load_snippets().ok();
        }
    }

    /// 刷新快捷命令配置
    #[allow(dead_code)]
    pub fn refresh_snippets_config(&mut self) {
        self.snippets_config = crate::services::storage::load_snippets().ok();
    }

    /// 确保命令输入框已创建
    pub fn ensure_command_input_created(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_input.is_none() {
            let lang = crate::services::storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or(crate::models::settings::Language::Chinese);
            let placeholder = crate::i18n::t(&lang, "session.terminal.command_placeholder");

            self.command_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 20) // 1-20 行自动增长，支持多行输入
            }));
        }
    }

    /// 初始化终端（在 UI 挂载并获取尺寸后调用）
    pub fn initialize_terminal(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // 查找 tab
        let tab = match self.tabs.iter_mut().find(|t| t.id == tab_id) {
            Some(t) => t,
            None => return,
        };

        // 检查是否已初始化或未连接
        if tab.pty_initialized || tab.status != SessionStatus::Connected {
            return;
        }

        info!("[Terminal] Initializing PTY for tab {}", tab_id);
        debug!(
            "[Terminal] Area size: {}x{} pixels",
            area_width, area_height
        );

        // 创建终端设置
        let settings = crate::services::storage::load_settings()
            .unwrap_or_default()
            .terminal;

        // 创建 TerminalState
        let terminal_state = cx.new(|_cx| crate::terminal::TerminalState::new(settings.clone()));

        // 计算终端尺寸
        let (cols, rows, cell_width, line_height) = crate::terminal::calculate_terminal_size(
            area_width,
            area_height,
            &settings,
            window,
            cx,
        );
        debug!(
            "[Terminal] Calculated size: {}x{} (cols x rows)",
            cols, rows
        );

        // 初始化终端尺寸
        terminal_state.update(cx, |t, _| {
            t.resize(area_width, area_height, cell_width, line_height);
        });

        // 存储终端状态
        tab.terminal = Some(terminal_state.clone());
        tab.pty_initialized = true;

        // 获取 session_id（tab.id 就是 session_id）
        let session_id = tab.id.clone();

        // 创建 PTY 请求（使用已计算的 cols/rows）
        let pty_request = crate::terminal::create_pty_request(cols, rows, area_width, area_height);

        // 异步创建 PTY channel (使用 App::spawn)
        let terminal_for_task = terminal_state.clone();
        let session_state_for_task = cx.entity().clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                // 获取 SSH session
                let session =
                    match crate::ssh::manager::SshManager::global().get_session(&session_id) {
                        Some(s) => s,
                        None => {
                            error!("[Terminal] No SSH session found for {}", session_id);
                            return;
                        }
                    };

                // 打开终端通道
                match session.open_terminal(pty_request).await {
                    Ok(channel) => {
                        let channel = std::sync::Arc::new(channel);
                        info!("[Terminal] PTY channel created for {}", session_id);

                        // 存储 channel 到 tab
                        let channel_for_state = channel.clone();
                        let session_id_for_state = session_id.clone();
                        let _ = async_cx.update(|cx| {
                            session_state_for_task.update(cx, |state, _| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_state)
                                {
                                    tab.pty_channel = Some(channel_for_state);
                                }
                            });
                        });

                        // 启动 PTY 读取循环
                        let _ = async_cx.update(|cx| {
                            crate::terminal::start_pty_reader(channel, terminal_for_task, cx);
                        });

                        debug!("[Terminal] PTY reader started for {}", session_id);
                    }
                    Err(e) => {
                        error!("[Terminal] Failed to open PTY: {:?}", e);
                    }
                }
            })
            .detach();

        cx.notify();
    }
}
