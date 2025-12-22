// HomePage 主页组件

use gpui::*;
use std::collections::HashMap;
use tracing::{debug, info};

use super::known_hosts_list::{render_known_hosts_content, KnownHostsPageState};
use super::server_list::{render_hosts_content, render_placeholder, ViewMode, ViewModeState};
use super::sidebar::{render_sidebar, MenuType, SidebarState};
use super::snippets_list::{render_snippets_content, SnippetsPageState};
use super::titlebar::{render_home_button, render_session_titlebar, render_titlebar};
use crate::components::common::server_dialog::{render_server_dialog_overlay, ServerDialogState};
use crate::components::common::settings_dialog::{
    render_settings_dialog_overlay, SettingsDialogState,
};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{HistoryItem, Server, ServerGroup};
use crate::pages::connecting::{render_connecting_page, ConnectingProgress};
use crate::pages::session::render_session_layout;
use crate::services::storage;
use crate::ssh::start_ssh_connection;
use crate::state::{SessionState, SessionStatus};

/// 主页状态
pub struct HomePage {
    pub server_groups: Vec<ServerGroup>,
    pub history: Vec<HistoryItem>,
    pub sidebar_state: Entity<SidebarState>,
    pub view_mode_state: Entity<ViewModeState>,
    pub dialog_state: Entity<ServerDialogState>,
    pub settings_dialog_state: Entity<SettingsDialogState>,
    pub session_state: Entity<SessionState>,
    pub snippets_state: Entity<SnippetsPageState>,
    pub known_hosts_state: Entity<KnownHostsPageState>,
    // 连接进度状态（按 tab_id 索引）
    pub connecting_progress: HashMap<String, Entity<ConnectingProgress>>,
    /// 上一次的 show_home 状态，用于检测视图切换
    last_show_home: bool,
}

impl HomePage {
    pub fn new(cx: &mut App) -> Self {
        let sidebar_state = cx.new(|_| SidebarState {
            selected_menu: MenuType::Hosts,
        });

        let view_mode_state = cx.new(|_| ViewModeState {
            mode: ViewMode::List,
        });

        let dialog_state = cx.new(|_| ServerDialogState::default());
        let settings_dialog_state = cx.new(|_| SettingsDialogState::default());
        let session_state = cx.new(|_| SessionState::default());
        let snippets_state = cx.new(|cx| SnippetsPageState::new(cx));
        let known_hosts_state = cx.new(|_| KnownHostsPageState::new());

        // 从存储加载服务器数据
        let server_groups = Self::load_server_groups();

        Self {
            server_groups,
            history: Self::load_history(),
            sidebar_state,
            view_mode_state,
            dialog_state,
            settings_dialog_state,
            session_state,
            snippets_state,
            known_hosts_state,
            connecting_progress: HashMap::new(),
            last_show_home: true,
        }
    }

    /// 从存储加载服务器分组数据
    fn load_server_groups() -> Vec<ServerGroup> {
        // 加载当前语言
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        let config = crate::services::storage::load_servers().unwrap_or_default();

        // 将 ServerData 转换为视图用的 Server 结构
        let mut server_groups: Vec<ServerGroup> = config
            .groups
            .iter()
            .map(|group| {
                let group_servers: Vec<Server> = config
                    .servers
                    .iter()
                    .filter(|s| s.group_id.as_deref() == Some(&group.id))
                    .map(|s| Server {
                        id: s.id.clone(),
                        name: s.label.clone(),
                        host: s.host.clone(),
                        port: s.port,
                        description: s.description.clone().unwrap_or_else(|| "-".into()),
                        account: s.username.clone(),
                        last_connected: s.last_connected_at.clone().unwrap_or_else(|| {
                            i18n::t(&lang, "server_list.never_connected").to_string()
                        }),
                    })
                    .collect();

                ServerGroup {
                    name: group.name.clone(),
                    icon_path: icons::SERVER,
                    servers: group_servers,
                }
            })
            .collect();

        // 未分组的服务器放入 "未分组" 分组
        let ungrouped_servers: Vec<Server> = config
            .servers
            .iter()
            .filter(|s| s.group_id.is_none())
            .map(|s| Server {
                id: s.id.clone(),
                name: s.label.clone(),
                host: s.host.clone(),
                port: s.port,
                description: s.description.clone().unwrap_or_else(|| "-".into()),
                account: s.username.clone(),
                last_connected: s
                    .last_connected_at
                    .clone()
                    .unwrap_or_else(|| i18n::t(&lang, "server_list.never_connected").to_string()),
            })
            .collect();

        if !ungrouped_servers.is_empty() {
            server_groups.push(ServerGroup {
                name: i18n::t(&lang, "server_list.ungrouped").to_string(),
                icon_path: icons::SERVER,
                servers: ungrouped_servers,
            });
        }

        server_groups
    }

    /// 重新加载服务器列表
    pub fn reload_servers(&mut self) {
        self.server_groups = Self::load_server_groups();
        self.history = Self::load_history();
    }

    /// 从存储加载历史记录
    fn load_history() -> Vec<HistoryItem> {
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        let config = storage::load_servers().unwrap_or_default();
        let mut servers_with_time: Vec<(String, chrono::NaiveDateTime)> = config
            .servers
            .iter()
            .filter_map(|s| {
                s.last_connected_at.as_ref().and_then(|time_str| {
                    match chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M") {
                        Ok(dt) => Some((s.label.clone(), dt)),
                        Err(_) => None,
                    }
                })
            })
            .collect();

        // 按时间倒序排序
        servers_with_time.sort_by(|a, b| b.1.cmp(&a.1));

        // 取前 10 条
        servers_with_time
            .into_iter()
            .take(10)
            .map(|(name, time)| {
                let now = chrono::Local::now().naive_local();
                let duration = now.signed_duration_since(time);
                let time_str = if duration.num_seconds() < 60 {
                    i18n::t(&lang, "history.just_now").to_string()
                } else if duration.num_minutes() < 60 {
                    format!(
                        "{}{}",
                        duration.num_minutes(),
                        i18n::t(&lang, "history.minutes_ago")
                    )
                } else if duration.num_hours() < 24 {
                    format!(
                        "{}{}",
                        duration.num_hours(),
                        i18n::t(&lang, "history.hours_ago")
                    )
                } else {
                    format!(
                        "{}{}",
                        duration.num_days(),
                        i18n::t(&lang, "history.days_ago")
                    )
                };

                HistoryItem {
                    name,
                    time: time_str,
                }
            })
            .collect()
    }

    fn render_content(&self, selected_menu: MenuType, cx: &Context<Self>) -> AnyElement {
        // 加载当前语言
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        let view_mode = self.view_mode_state.read(cx).mode;
        match selected_menu {
            MenuType::Hosts => render_hosts_content(
                &self.server_groups,
                view_mode,
                self.view_mode_state.clone(),
                self.dialog_state.clone(),
                self.session_state.clone(),
                cx,
            )
            .into_any_element(),
            MenuType::Monitor => {
                render_placeholder("Monitor", i18n::t(&lang, "session.monitor.title"), cx)
                    .into_any_element()
            }
            MenuType::Snippets => {
                render_snippets_content(self.snippets_state.clone(), cx).into_any_element()
            }
            MenuType::KnownHosts => {
                render_known_hosts_content(self.known_hosts_state.clone(), cx).into_any_element()
            }
        }
    }

    /// 渲染主页视图（Sidebar + 服务器列表）
    fn render_home_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let history = self.history.clone();
        let sidebar_state = self.sidebar_state.clone();
        let selected_menu = self.sidebar_state.read(cx).selected_menu;
        let dialog_visible = self.dialog_state.read(cx).visible;
        let settings_dialog_visible = self.settings_dialog_state.read(cx).visible;
        let dialog_state = self.dialog_state.clone();
        let settings_dialog_state = self.settings_dialog_state.clone();

        // 检查是否有会话，决定使用哪个标题栏
        let has_sessions = self.session_state.read(cx).has_sessions();
        let session_state = self.session_state.clone();

        div()
            .size_full()
            .bg(crate::theme::background_color(cx))
            .flex()
            .flex_col()
            .relative()
            // 第一行：Home 按钮区域 + Titlebar
            .child(
                div()
                    .w_full()
                    .flex()
                    // Home 按钮区域（独立，宽度与 sidebar 相同）
                    .child(render_home_button(session_state.clone(), cx))
                    // Titlebar（有会话时显示标签页）
                    .child(if has_sessions {
                        render_session_titlebar(session_state, cx).into_any_element()
                    } else {
                        render_titlebar(cx).into_any_element()
                    }),
            )
            // 第二行：Sidebar + Content
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .overflow_hidden() // 防止内容溢出，确保设置按钮可见
                    .flex()
                    // Sidebar
                    .child(render_sidebar(
                        sidebar_state,
                        selected_menu,
                        &history,
                        self.settings_dialog_state.clone(),
                        cx,
                    ))
                    // Content
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .child(self.render_content(selected_menu, cx)),
                    ),
            )
            // 服务器弹窗
            .children(if dialog_visible {
                self.dialog_state.update(cx, |state, cx| {
                    state.ensure_inputs_created(window, cx);
                });
                Some(render_server_dialog_overlay(dialog_state, cx))
            } else {
                None
            })
            // 设置弹窗
            .children(if settings_dialog_visible {
                self.settings_dialog_state.update(cx, |state, cx| {
                    state.ensure_inputs_created(window, cx);
                });
                Some(render_settings_dialog_overlay(settings_dialog_state, cx))
            } else {
                None
            })
            // Snippets 弹窗
            .children({
                let snippets_dialog_open =
                    self.snippets_state.read(cx).dialog_state.read(cx).is_open();
                if snippets_dialog_open {
                    let dialog_state = self.snippets_state.read(cx).dialog_state.clone();
                    dialog_state.update(cx, |state, cx| {
                        state.ensure_inputs_created(window, cx);
                    });
                    Some(
                        crate::components::common::snippets_dialog::render_snippets_dialog_overlay(
                            dialog_state,
                            cx,
                        ),
                    )
                } else {
                    None
                }
            })
    }

    /// 渲染会话视图（标签页 + 内容）
    fn render_session_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let session_state = self.session_state.clone();
        let state = session_state.read(cx);

        // 获取当前活动标签
        let active_tab = state.active_tab().cloned();

        // 渲染内容区域
        let content: AnyElement = if let Some(tab) = active_tab {
            match &tab.status {
                SessionStatus::Connecting => {
                    // 获取或创建连接进度状态
                    let progress_state = self
                        .connecting_progress
                        .entry(tab.id.clone())
                        .or_insert_with(|| cx.new(|_| ConnectingProgress::new(tab.id.clone())))
                        .clone();

                    // 检查是否需要启动连接（首次进入此 tab 时启动）
                    let should_start = !progress_state.read(cx).connection_started;

                    if should_start {
                        let server_label = tab.server_label.clone();
                        info!("[SSH] 开始连接到服务器: {}", server_label);

                        // 标记连接已启动
                        progress_state.update(cx, |p, _| {
                            p.mark_started();
                        });

                        // 启动 SSH 连接（使用 SSH 模块）
                        let progress_for_timer = progress_state.clone();
                        let session_for_timer = session_state.clone();
                        let tab_id = tab.id.clone();
                        let server_id_for_log = tab.server_id.clone();

                        // 根据 server_id 获取完整的 ServerData
                        if let Ok(config) = crate::services::storage::load_servers() {
                            if let Some(server_data) = config
                                .servers
                                .iter()
                                .find(|s| s.id == server_id_for_log)
                                .cloned()
                            {
                                start_ssh_connection(
                                    server_data,
                                    tab_id,
                                    progress_for_timer,
                                    session_for_timer,
                                    cx,
                                );
                            }
                        }
                    }

                    render_connecting_page(&tab, progress_state, session_state.clone(), cx)
                        .into_any_element()
                }
                SessionStatus::Connected => {
                    // 确保命令输入框已创建
                    session_state.update(cx, |state, cx| {
                        state.ensure_command_input_created(window, cx);
                    });

                    // 确保 SFTP 文件列表视图已创建并同步数据
                    let tab_id_for_sftp = tab.id.clone();
                    session_state.update(cx, |state, cx| {
                        let view = state.ensure_sftp_file_list_view(&tab_id_for_sftp, window, cx);
                        // 同步 SFTP 状态到 FileListView
                        let sftp_state = state
                            .tabs
                            .iter()
                            .find(|t| t.id == tab_id_for_sftp)
                            .and_then(|t| t.sftp_state.as_ref());
                        view.update(cx, |v, cx| {
                            v.sync_from_sftp_state(sftp_state, cx);
                        });

                        // 提取当前路径（避免借用冲突）
                        let current_path = sftp_state.map(|s| s.current_path.clone());

                        // 确保 PathBarState 创建并同步路径
                        let entity = cx.entity().clone();
                        let path_bar =
                            state.ensure_sftp_path_bar_state(&tab_id_for_sftp, entity, window, cx);
                        // 同步当前路径到 PathBarState
                        if let Some(path) = current_path {
                            path_bar.update(cx, |pb, cx| {
                                pb.set_path(&path, window, cx);
                            });
                        }
                    });

                    // 检查当前激活的终端是否已初始化
                    let needs_init = tab
                        .active_terminal_id
                        .as_ref()
                        .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
                        .map(|inst| !inst.pty_initialized)
                        .unwrap_or(false);

                    // 自动初始化 PTY（在 UI 挂载成功后触发）
                    if needs_init {
                        let tab_id = tab.id.clone();
                        let sidebar_collapsed = session_state.read(cx).sidebar_collapsed;
                        session_state.update(cx, |state, cx| {
                            // 根据窗口尺寸计算终端区域
                            // 布局常量（与 session_layout.rs 保持一致）
                            const TITLEBAR_HEIGHT: f32 = 44.0;
                            const MONITOR_PANEL_WIDTH: f32 = 230.0;
                            const SFTP_PANEL_HEIGHT: f32 = 300.0;
                            const SIDEBAR_WIDTH: f32 = 230.0;
                            const MINI_SIDEBAR_WIDTH: f32 = 28.0;
                            const COMMAND_INPUT_HEIGHT: f32 = 40.0; // 24px 按钮 + 2*8px padding

                            // 获取窗口内容区域尺寸
                            let window_size = window.viewport_size();
                            let window_width = f32::from(window_size.width);
                            let window_height = f32::from(window_size.height);

                            // 计算终端区域尺寸
                            let sidebar_width = if sidebar_collapsed {
                                0.0
                            } else {
                                SIDEBAR_WIDTH
                            };
                            let terminal_width = window_width
                                - MONITOR_PANEL_WIDTH
                                - sidebar_width
                                - MINI_SIDEBAR_WIDTH;
                            let terminal_height = window_height
                                - TITLEBAR_HEIGHT
                                - SFTP_PANEL_HEIGHT
                                - COMMAND_INPUT_HEIGHT;

                            debug!(
                                "[Terminal] Window: {}x{}, Calculated terminal area: {}x{}",
                                window_width, window_height, terminal_width, terminal_height
                            );

                            state.initialize_terminal(
                                &tab_id,
                                terminal_width.max(100.0),
                                terminal_height.max(100.0),
                                window,
                                cx,
                            );
                        });
                    }

                    // 确保 SFTP 新建文件夹对话框输入框已创建
                    let new_folder_dialog = session_state.read(cx).sftp_new_folder_dialog.clone();
                    if let Some(dialog) = new_folder_dialog {
                        let is_open = dialog.read(cx).is_open;
                        if is_open {
                            dialog.update(cx, |ds, cx| {
                                ds.ensure_input_created(window, cx);
                            });
                        }
                    }

                    let sidebar_collapsed = session_state.read(cx).sidebar_collapsed;
                    render_session_layout(
                        &tab,
                        sidebar_collapsed,
                        session_state.clone(),
                        window,
                        cx,
                    )
                    .into_any_element()
                }
                SessionStatus::Error(_) | SessionStatus::Disconnected => {
                    // 错误或断开状态也使用连接页面显示
                    let progress_state = self
                        .connecting_progress
                        .entry(tab.id.clone())
                        .or_insert_with(|| cx.new(|_| ConnectingProgress::new(tab.id.clone())))
                        .clone();

                    render_connecting_page(&tab, progress_state, session_state.clone(), cx)
                        .into_any_element()
                }
            }
        } else {
            // 没有活动标签，显示空白
            div().into_any_element()
        };

        div()
            .size_full()
            .bg(crate::theme::background_color(cx))
            .flex()
            .flex_col()
            // 第一行：Home 按钮区域 + 会话标题栏
            .child(
                div()
                    .w_full()
                    .flex()
                    // Home 按钮区域
                    .child(render_home_button(session_state.clone(), cx))
                    // 会话标题栏（带标签页）
                    .child(render_session_titlebar(session_state, cx)),
            )
            // 第二行：会话内容区域
            .child(div().flex_1().w_full().min_h(px(0.)).child(content))
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 统一的服务器列表刷新逻辑
        let show_home = self.session_state.read(cx).show_home;
        let needs_refresh_from_dialog = self.dialog_state.read(cx).needs_refresh;

        // 刷新条件：1) 从会话视图切换到主页视图  2) 对话框保存后需要刷新
        if (show_home && !self.last_show_home) || needs_refresh_from_dialog {
            self.reload_servers();
            if needs_refresh_from_dialog {
                self.dialog_state.update(cx, |state, _| {
                    state.needs_refresh = false;
                });
            }
        }
        self.last_show_home = show_home;

        // Snippets 弹窗保存后刷新
        let snippets_needs_refresh = self
            .snippets_state
            .read(cx)
            .dialog_state
            .read(cx)
            .needs_page_refresh;
        if snippets_needs_refresh {
            // 刷新 Home 页面的 SnippetsPageState
            self.snippets_state.update(cx, |state, cx| {
                state.refresh();
                state.dialog_state.update(cx, |ds, _| {
                    ds.needs_page_refresh = false;
                });
            });
            // 同时刷新 Session 页面的 snippets_config
            self.session_state.update(cx, |state, _| {
                state.refresh_snippets_config();
            });
        }

        // 清理已关闭标签的进度状态
        let active_tabs: Vec<String> = self
            .session_state
            .read(cx)
            .tabs
            .iter()
            .map(|t| t.id.clone())
            .collect();
        self.connecting_progress
            .retain(|id, _| active_tabs.contains(id));

        // 根据 show_home 状态决定渲染哪个视图
        let has_sessions = self.session_state.read(cx).has_sessions();

        // 如果 show_home=true 或没有会话，显示主页
        // 如果 show_home=false 且有会话，显示会话视图
        if show_home || !has_sessions {
            self.render_home_view(window, cx).into_any_element()
        } else {
            self.render_session_view(window, cx).into_any_element()
        }
    }
}
