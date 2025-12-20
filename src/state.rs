// 全局 AppState

use crate::components::monitor::DetailDialogState;
use crate::models::monitor::MonitorState;
use crate::models::sftp::SftpState;
use crate::models::SnippetsConfig;
use crate::services::monitor::{MonitorEvent, MonitorService, MonitorSettings};
use crate::services::sftp::SftpService;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::prelude::*;
use gpui::{Entity, FocusHandle};
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
        }
    }
}

impl SessionState {
    /// 添加新的会话标签（插入到最前面）
    pub fn add_tab(&mut self, server_id: String, server_label: String) -> String {
        let tab_id = uuid::Uuid::new_v4().to_string();

        // 创建第一个终端实例
        let first_terminal = TerminalInstance {
            id: uuid::Uuid::new_v4().to_string(),
            index: 1,
            terminal: None,
            pty_channel: None,
            pty_initialized: false,
            last_sent_pty_size: None,
            pty_error: None,
        };
        let first_terminal_id = first_terminal.id.clone();

        let tab = SessionTab {
            id: tab_id.clone(),
            server_id,
            server_label,
            status: SessionStatus::Connecting,
            terminals: vec![first_terminal],
            active_terminal_id: Some(first_terminal_id),
            terminal_counter: 1,
            monitor_state: MonitorState::empty(),
            sftp_state: None,
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

            // 停止并移除 MonitorService（Drop 会自动调用 stop）
            if let Ok(mut services) = self.monitor_services.lock() {
                if services.remove(tab_id).is_some() {
                    info!("[Monitor] Service removed for closed tab {}", tab_id);
                }
            }

            // 关闭 SSH 会话
            let ssh_manager = crate::ssh::manager::SshManager::global();
            ssh_manager.close_session(tab_id);
            info!("[Session] SSH session closed for tab {}", tab_id);
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
    pub fn refresh_snippets_config(&mut self) {
        self.snippets_config = crate::services::storage::load_snippets().ok();
    }

    /// 确保 Monitor 详情弹窗状态已创建
    pub fn ensure_monitor_detail_dialog(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<DetailDialogState> {
        if self.monitor_detail_dialog.is_none() {
            self.monitor_detail_dialog = Some(cx.new(|_| DetailDialogState::default()));
        }
        self.monitor_detail_dialog.clone().unwrap()
    }

    /// 确保命令输入框已创建，并更新占位符为当前语言
    pub fn ensure_command_input_created(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let lang = crate::services::storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(crate::models::settings::Language::Chinese);
        let placeholder = crate::i18n::t(&lang, "session.terminal.command_placeholder");

        if self.command_input.is_none() {
            // 首次创建
            self.command_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 20) // 1-20 行自动增长，支持多行输入
            }));
        } else {
            // 更新占位符（语言可能已变化）
            if let Some(input) = &self.command_input {
                input.update(cx, |state, cx| {
                    state.set_placeholder(placeholder, window, cx);
                });
            }
        }
    }

    /// 设置命令输入框的文本内容
    pub fn set_command_input_text(
        &self,
        text: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(input) = &self.command_input {
            input.update(cx, |state, cx| {
                state.set_value(text, window, cx);
            });
        }
    }

    /// 确保终端焦点句柄已创建
    pub fn ensure_terminal_focus_handle_created(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> FocusHandle {
        if self.terminal_focus_handle.is_none() {
            self.terminal_focus_handle = Some(cx.focus_handle());
        }
        self.terminal_focus_handle.clone().unwrap()
    }

    /// 获取终端焦点句柄（如果存在）
    pub fn get_terminal_focus_handle(&self) -> Option<FocusHandle> {
        self.terminal_focus_handle.clone()
    }

    /// 初始化终端（在 UI 挂载并获取尺寸后调用）
    /// 只初始化当前激活的终端实例
    pub fn initialize_terminal(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // 先确保终端焦点句柄已创建（在任何可变借用之前）
        self.ensure_terminal_focus_handle_created(cx);

        // 查找 tab 并检查状态
        let tab_id_owned = tab_id.to_string();

        // 获取需要初始化的终端实例 ID
        let terminal_instance_id = {
            let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else {
                return;
            };
            if tab.status != SessionStatus::Connected {
                return;
            }
            let Some(active_id) = &tab.active_terminal_id else {
                return;
            };
            let Some(instance) = tab.terminals.iter().find(|t| &t.id == active_id) else {
                return;
            };
            if instance.pty_initialized {
                return;
            }
            active_id.clone()
        };

        info!(
            "[Terminal] Initializing PTY for tab {} terminal {}",
            tab_id, terminal_instance_id
        );
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

        // 存储终端状态到对应的终端实例
        let terminal_instance_id_for_store = terminal_instance_id.clone();
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id_owned) {
            if let Some(instance) = tab
                .terminals
                .iter_mut()
                .find(|t| t.id == terminal_instance_id_for_store)
            {
                instance.terminal = Some(terminal_state.clone());
                instance.pty_initialized = true;
                instance.last_sent_pty_size = Some((cols, rows));
            }
        }

        // 启动光标闪烁定时器 (500ms 间隔)
        let terminal_for_blink = terminal_state.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                loop {
                    // 等待 500ms
                    async_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;

                    // 切换光标可见性
                    let result = async_cx.update(|cx| {
                        terminal_for_blink.update(cx, |t, cx| {
                            t.toggle_cursor_visibility();
                            cx.notify();
                        });
                    });

                    // 如果更新失败（例如终端已关闭），退出循环
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();

        // 创建 PTY 请求（使用已计算的 cols/rows）
        let pty_request = crate::terminal::create_pty_request(cols, rows, area_width, area_height);

        // 异步创建 PTY channel (使用 App::spawn)
        let terminal_for_task = terminal_state.clone();
        let session_state_for_task = cx.entity().clone();
        let session_id = tab_id_owned.clone();
        let terminal_id_for_task = terminal_instance_id.clone();
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
                        info!(
                            "[Terminal] PTY channel created for {} terminal {}",
                            session_id, terminal_id_for_task
                        );

                        // 存储 channel 到终端实例
                        let channel_for_state = channel.clone();
                        let session_id_for_state = session_id.clone();
                        let terminal_id_for_state = terminal_id_for_task.clone();
                        let _ = async_cx.update(|cx| {
                            session_state_for_task.update(cx, |state, _| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_state)
                                {
                                    if let Some(instance) = tab
                                        .terminals
                                        .iter_mut()
                                        .find(|t| t.id == terminal_id_for_state)
                                    {
                                        instance.pty_channel = Some(channel_for_state);
                                    }
                                }
                            });
                        });

                        // 启动 PTY 读取循环
                        let _ = async_cx.update(|cx| {
                            crate::terminal::start_pty_reader(channel, terminal_for_task, cx);
                        });

                        debug!(
                            "[Terminal] PTY reader started for {} terminal {}",
                            session_id, terminal_id_for_task
                        );
                    }
                    Err(e) => {
                        error!("[Terminal] Failed to open PTY: {:?}", e);
                        // 记录错误到终端实例
                        let session_id_for_err = session_id.clone();
                        let terminal_id_for_err = terminal_id_for_task.clone();
                        let error_msg = format!("{:?}", e);
                        let _ = async_cx.update(|cx| {
                            session_state_for_task.update(cx, |state, _| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_err)
                                {
                                    if let Some(instance) = tab
                                        .terminals
                                        .iter_mut()
                                        .find(|t| t.id == terminal_id_for_err)
                                    {
                                        instance.pty_error = Some(error_msg);
                                    }
                                }
                            });
                        });
                    }
                }
            })
            .detach();

        cx.notify();
    }

    /// 将本地终端尺寸与远端 PTY 尺寸同步到给定像素区域（用于窗口/布局变化时的自动 resize）
    /// 只同步当前激活的终端实例
    /// 注意：初始化由单独的机制触发，此方法仅处理 resize
    pub fn sync_terminal_size(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        if area_width <= 0.0 || area_height <= 0.0 {
            return;
        }

        // 获取当前激活的终端实例信息
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        if tab.status != SessionStatus::Connected {
            return;
        }

        let Some(active_id) = tab.active_terminal_id.clone() else {
            return;
        };

        let Some(instance) = tab.terminals.iter_mut().find(|t| t.id == active_id) else {
            return;
        };

        // 如果未初始化，跳过（初始化由其他机制触发）
        if !instance.pty_initialized {
            return;
        }

        let (Some(terminal), channel, last_sent) = (
            instance.terminal.clone(),
            instance.pty_channel.clone(),
            instance.last_sent_pty_size,
        ) else {
            return;
        };

        let (cell_width, line_height) = {
            let size = terminal.read(cx).size();
            (size.cell_width, size.line_height)
        };

        let new_size = crate::terminal::TerminalSize::from_pixels(
            area_width,
            area_height,
            cell_width,
            line_height,
        );
        let cols = new_size.columns as u32;
        let rows = new_size.lines as u32;

        terminal.update(cx, |t, _| {
            t.resize(area_width, area_height, cell_width, line_height);
        });

        let Some(channel) = channel else {
            return;
        };

        if last_sent == Some((cols, rows)) {
            return;
        }

        instance.last_sent_pty_size = Some((cols, rows));
        let channel_for_resize = channel.clone();
        cx.to_async()
            .spawn(async move |_async_cx| {
                if let Err(e) = channel_for_resize.resize(cols, rows).await {
                    error!("[Terminal] Failed to resize PTY: {:?}", e);
                }
            })
            .detach();
    }

    /// 添加新的终端实例到指定会话标签
    /// 返回新终端实例的 ID
    pub fn add_terminal_instance(&mut self, tab_id: &str) -> Option<String> {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return None;
        };

        tab.terminal_counter += 1;
        let new_instance = TerminalInstance {
            id: uuid::Uuid::new_v4().to_string(),
            index: tab.terminal_counter,
            terminal: None,
            pty_channel: None,
            pty_initialized: false,
            last_sent_pty_size: None,
            pty_error: None,
        };
        let new_id = new_instance.id.clone();
        tab.terminals.push(new_instance);
        tab.active_terminal_id = Some(new_id.clone());

        info!(
            "[Terminal] Added new terminal instance {} to tab {}",
            new_id, tab_id
        );
        Some(new_id)
    }

    /// 关闭指定的终端实例
    pub fn close_terminal_instance(&mut self, tab_id: &str, terminal_id: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        // 不允许关闭最后一个终端实例
        if tab.terminals.len() <= 1 {
            return;
        }

        if let Some(pos) = tab.terminals.iter().position(|t| t.id == terminal_id) {
            tab.terminals.remove(pos);

            // 如果关闭的是当前激活的终端，切换到第一个
            if tab.active_terminal_id.as_deref() == Some(terminal_id) {
                tab.active_terminal_id = tab.terminals.first().map(|t| t.id.clone());
            }

            info!(
                "[Terminal] Closed terminal instance {} from tab {}",
                terminal_id, tab_id
            );
        }
    }

    /// 激活指定的终端实例
    pub fn activate_terminal_instance(&mut self, tab_id: &str, terminal_id: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        if tab.terminals.iter().any(|t| t.id == terminal_id) {
            tab.active_terminal_id = Some(terminal_id.to_string());
            debug!(
                "[Terminal] Activated terminal instance {} in tab {}",
                terminal_id, tab_id
            );
        }
    }

    /// 获取当前激活的终端实例
    pub fn active_terminal_instance(&self, tab_id: &str) -> Option<&TerminalInstance> {
        let tab = self.tabs.iter().find(|t| t.id == tab_id)?;
        let active_id = tab.active_terminal_id.as_ref()?;
        tab.terminals.iter().find(|t| &t.id == active_id)
    }

    /// 获取指定标签的所有终端实例
    pub fn get_terminal_instances(&self, tab_id: &str) -> Vec<&TerminalInstance> {
        self.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|tab| tab.terminals.iter().collect())
            .unwrap_or_default()
    }

    /// 获取指定标签的当前激活终端 ID
    pub fn active_terminal_id(&self, tab_id: &str) -> Option<String> {
        self.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|tab| tab.active_terminal_id.clone())
    }

    /// 启动 Monitor 服务
    /// 在 SSH 连接成功后调用，开始收集服务器监控数据
    pub fn start_monitor_service(&self, tab_id: String, cx: &mut gpui::Context<Self>) {
        let session_state = cx.entity().clone();

        // 获取 SSH manager 和 session
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let Some(session) = ssh_manager.get_session(&tab_id) else {
            error!("[Monitor] No SSH session found for tab {}", tab_id);
            return;
        };

        info!("[Monitor] Starting monitor service for tab {}", tab_id);

        // 创建 MonitorService 并获取事件接收器
        // 使用 SSH manager 的 tokio 运行时来启动异步任务
        let (service, mut event_rx) = MonitorService::new(
            tab_id.clone(),
            session,
            MonitorSettings::default(),
            ssh_manager.runtime(),
        );

        // 将 service 存入 HashMap 以保持其生命周期
        if let Ok(mut services) = self.monitor_services.lock() {
            services.insert(tab_id.clone(), service);
        }

        // 在 SSH 运行时中启动 Monitor 事件处理任务
        let tab_id_for_task = tab_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                info!("[Monitor] Event loop started for tab {}", tab_id_for_task);

                while let Some(event) = event_rx.recv().await {
                    let tab_id_clone = tab_id_for_task.clone();

                    let result = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                            {
                                match event.clone() {
                                    MonitorEvent::SystemInfo(info) => {
                                        debug!(
                                            "[Monitor] Received SystemInfo for tab {}",
                                            tab_id_clone
                                        );
                                        tab.monitor_state.update_system_info(info);
                                    }
                                    MonitorEvent::LoadInfo(info) => {
                                        debug!(
                                            "[Monitor] Received LoadInfo for tab {}",
                                            tab_id_clone
                                        );
                                        tab.monitor_state.update_load_info(info);
                                    }
                                    MonitorEvent::NetworkInfo(info) => {
                                        debug!(
                                            "[Monitor] Received NetworkInfo for tab {}",
                                            tab_id_clone
                                        );
                                        tab.monitor_state.update_network_info(info);
                                    }
                                    MonitorEvent::DiskInfo(info) => {
                                        debug!(
                                            "[Monitor] Received DiskInfo for tab {}",
                                            tab_id_clone
                                        );
                                        tab.monitor_state.update_disk_info(info);
                                    }
                                    MonitorEvent::Error(msg) => {
                                        error!("[Monitor] Error for tab {}: {}", tab_id_clone, msg);
                                    }
                                }
                                cx.notify();
                            }
                        });
                    });

                    if result.is_err() {
                        info!(
                            "[Monitor] Session state no longer available for tab {}",
                            tab_id_for_task
                        );
                        break;
                    }
                }

                info!("[Monitor] Event loop ended for tab {}", tab_id_for_task);
            })
            .detach();
    }

    /// 启动 SFTP 服务
    /// 在 SSH 连接成功后调用，初始化 SFTP 子系统并加载用户主目录
    pub fn start_sftp_service(&mut self, tab_id: String, cx: &mut gpui::Context<Self>) {
        let session_state = cx.entity().clone();

        // 获取 SSH manager 和 session
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let Some(session) = ssh_manager.get_session(&tab_id) else {
            error!("[SFTP] No SSH session found for tab {}", tab_id);
            return;
        };

        info!("[SFTP] Starting SFTP service for tab {}", tab_id);

        // 直接初始化空的 SftpState（不需要嵌套 update）
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            let mut sftp_state = SftpState::default();
            sftp_state.show_hidden = true; // 默认显示隐藏文件
            tab.sftp_state = Some(sftp_state);
        }

        // 创建 channel 用于从 tokio 运行时发送结果到 GPUI
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SftpInitResult>();

        // 在 SSH 运行时中启动 SFTP 初始化任务
        let tab_id_for_tokio = tab_id.clone();
        let sftp_services = self.sftp_services.clone();
        ssh_manager.runtime().spawn(async move {
            // 获取 SFTP 子系统（在 tokio 运行时中）
            let sftp_result = SftpService::new(tab_id_for_tokio.clone(), &session).await;

            match sftp_result {
                Ok(service) => {
                    info!(
                        "[SFTP] SFTP service initialized for tab {}",
                        tab_id_for_tokio
                    );

                    // ============================================================
                    // 第一阶段：并行获取主目录、/etc/passwd、/etc/group
                    // ============================================================
                    let (home_result, passwd_result, group_result) = tokio::join!(
                        service.get_home_dir(),
                        service.read_file("/etc/passwd"),
                        service.read_file("/etc/group")
                    );

                    let home_dir = match home_result {
                        Ok(home) => {
                            info!("[SFTP] Home directory: {}", home);
                            home
                        }
                        Err(e) => {
                            error!("[SFTP] Failed to get home directory: {:?}", e);
                            "/".to_string()
                        }
                    };

                    let passwd_content = match passwd_result {
                        Ok(content) => {
                            info!("[SFTP] Loaded /etc/passwd ({} bytes)", content.len());
                            Some(content)
                        }
                        Err(e) => {
                            error!("[SFTP] Failed to read /etc/passwd: {}", e);
                            None
                        }
                    };

                    let group_content = match group_result {
                        Ok(content) => {
                            info!("[SFTP] Loaded /etc/group ({} bytes)", content.len());
                            Some(content)
                        }
                        Err(e) => {
                            error!("[SFTP] Failed to read /etc/group: {}", e);
                            None
                        }
                    };

                    // ============================================================
                    // 第二阶段：并行读取所有路径层级的目录内容
                    // ============================================================
                    let path_hierarchy = get_path_hierarchy(&home_dir);
                    info!("[SFTP] Path hierarchy: {:?}", path_hierarchy);

                    // 创建并行读取任务
                    let sftp = service.sftp();
                    let read_futures: Vec<_> = path_hierarchy
                        .iter()
                        .map(|path| {
                            let path = path.clone();
                            let sftp = sftp.clone();
                            async move {
                                let result = sftp.read_dir(&path).await;
                                (path, result)
                            }
                        })
                        .collect();

                    // 并行执行所有目录读取
                    let dir_results = futures::future::join_all(read_futures).await;

                    // 收集成功的结果
                    let mut dir_caches: Vec<(String, Vec<crate::models::sftp::FileEntry>)> =
                        Vec::new();
                    for (path, result) in dir_results {
                        match result {
                            Ok(entries) => {
                                // ReadDir 是迭代器，先收集成 Vec
                                let entries: Vec<_> = entries.collect();
                                // 转换为我们的 FileEntry 类型
                                let file_entries: Vec<crate::models::sftp::FileEntry> = entries
                                    .into_iter()
                                    .filter_map(|entry| {
                                        let name = entry.file_name();
                                        if name == "." || name == ".." {
                                            return None;
                                        }
                                        let full_path = if path == "/" {
                                            format!("/{}", name)
                                        } else {
                                            format!("{}/{}", path.trim_end_matches('/'), name)
                                        };
                                        let attrs = entry.metadata();
                                        let file_type = if attrs.is_dir() {
                                            crate::models::sftp::FileType::Directory
                                        } else if attrs.is_symlink() {
                                            crate::models::sftp::FileType::Symlink
                                        } else {
                                            crate::models::sftp::FileType::File
                                        };
                                        let mut file_entry = crate::models::sftp::FileEntry::new(
                                            name.to_string(),
                                            full_path,
                                            file_type,
                                        );
                                        file_entry.size = attrs.size.unwrap_or(0);
                                        file_entry.permissions =
                                            attrs.permissions.map(|p| p as u32).unwrap_or(0);
                                        file_entry.uid = attrs.uid;
                                        file_entry.gid = attrs.gid;
                                        if let Some(mtime) = attrs.mtime {
                                            file_entry.modified = Some(
                                                std::time::UNIX_EPOCH
                                                    + std::time::Duration::from_secs(mtime as u64),
                                            );
                                        }
                                        Some(file_entry)
                                    })
                                    .collect();
                                info!("[SFTP] Loaded {} entries from {}", file_entries.len(), path);
                                dir_caches.push((path, file_entries));
                            }
                            Err(e) => {
                                error!("[SFTP] Failed to read directory {}: {:?}", path, e);
                            }
                        }
                    }

                    // 存储 service
                    if let Ok(mut services) = sftp_services.lock() {
                        services.insert(tab_id_for_tokio.clone(), service);
                    }

                    // 发送成功结果
                    let _ = tx.send(SftpInitResult::Success {
                        home_dir,
                        path_hierarchy,
                        dir_caches,
                        passwd_content,
                        group_content,
                    });
                }
                Err(e) => {
                    error!("[SFTP] Failed to initialize SFTP service: {:?}", e);
                    let _ = tx.send(SftpInitResult::Error(format!("SFTP 初始化失败: {}", e)));
                }
            }
        });

        // 在 GPUI 异步上下文中等待结果并更新 UI
        let tab_id_for_ui = tab_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let tab_id_clone = tab_id_for_ui.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                            {
                                if let Some(sftp_state) = &mut tab.sftp_state {
                                    match result {
                                        SftpInitResult::Success {
                                            home_dir,
                                            path_hierarchy: _,
                                            dir_caches,
                                            passwd_content,
                                            group_content,
                                        } => {
                                            sftp_state.set_home_dir(home_dir.clone());
                                            sftp_state.navigate_to(home_dir.clone());

                                            // 解析用户/组信息
                                            if let Some(passwd) = passwd_content {
                                                sftp_state.parse_passwd(&passwd);
                                            }
                                            if let Some(group) = group_content {
                                                sftp_state.parse_group(&group);
                                            }

                                            // 缓存所有层级目录
                                            for (path, entries) in dir_caches {
                                                sftp_state
                                                    .update_cache(path.clone(), entries.clone());
                                                // 如果是 home 目录，更新文件列表
                                                if path == home_dir {
                                                    sftp_state.update_file_list(entries);
                                                }
                                            }

                                            // 自动展开路径层级到 home 目录
                                            sftp_state.expand_to_path(&home_dir);

                                            sftp_state.set_loading(false);
                                        }
                                        SftpInitResult::Error(msg) => {
                                            sftp_state.set_error(msg);
                                            sftp_state.set_loading(false);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    });
                }

                info!("[SFTP] Initialization task ended for tab {}", tab_id_for_ui);
            })
            .detach();
    }
}

/// SFTP 初始化结果
enum SftpInitResult {
    Success {
        home_dir: String,
        path_hierarchy: Vec<String>,
        dir_caches: Vec<(String, Vec<crate::models::sftp::FileEntry>)>,
        passwd_content: Option<String>,
        group_content: Option<String>,
    },
    Error(String),
}

/// 计算路径层级列表（如 /home/wuyun -> ["/", "/home", "/home/wuyun"]）
fn get_path_hierarchy(path: &str) -> Vec<String> {
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
