// HomePage 主页组件

use gpui_kit::*;
use std::collections::HashMap;
use tracing::info;

use super::known_hosts_list::{render_known_hosts_content, KnownHostsPageState};
use super::server_list::{
    render_delete_confirm, render_hosts_content, render_placeholder, HostsContent, ViewMode,
    ViewModeState,
};
use super::sidebar::{
    render_sidebar, GroupEntry, MenuType, NavCounts, SidebarProps, SidebarSection, SidebarState,
};
use super::snippets_list::{render_snippets_content, SnippetsPageState};
use super::titlebar::{render_home_button, render_home_topbar, render_session_titlebar};
use super::widgets::TextFitter;
use crate::components::common::icon::distribution_icon;
use crate::components::common::server_dialog::{render_server_dialog_overlay, ServerDialogState};
use crate::components::common::settings_dialog::{
    render_settings_dialog_overlay, SettingsDialogState,
};
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{HistoryItem, Server, ServerData, ServerGroup};
use crate::pages::connecting::{render_connecting_page, ConnectingProgress};
use crate::pages::session::render_session_layout;
use crate::services::storage;
use crate::ssh::start_ssh_connection;
use crate::state::{SessionState, SessionStatus, UpdateState};

/// 服务器连接时间在配置文件中的存储格式
const CONNECTED_AT_FORMAT: &str = "%Y-%m-%d %H:%M";
/// 「最近连接」保留的条数
const HISTORY_LIMIT: usize = 10;

/// 解析配置文件中保存的连接时间；格式不对时返回 `None`
fn parse_connected_at(value: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(value, CONNECTED_AT_FORMAT).ok()
}

/// 把持久化的服务器数据转换为视图用的结构
fn server_view(data: &ServerData) -> Server {
    Server {
        id: data.id.clone(),
        name: data.label.clone(),
        host: data.host.clone(),
        port: data.port,
        description: data.description.clone().unwrap_or_default(),
        account: data.username.clone(),
        os_name: data.os_name.clone(),
        last_connected_at: data
            .last_connected_at
            .as_deref()
            .and_then(parse_connected_at),
    }
}

/// 主页状态
pub struct HomePage {
    pub server_groups: Vec<ServerGroup>,
    pub history: Vec<HistoryItem>,
    pub sidebar_state: Entity<SidebarState>,
    pub view_mode_state: Entity<ViewModeState>,
    pub dialog_state: Entity<ServerDialogState>,
    pub settings_dialog_state: Entity<SettingsDialogState>,
    pub update_state: Entity<UpdateState>,
    pub session_state: Entity<SessionState>,
    pub snippets_state: Entity<SnippetsPageState>,
    pub known_hosts_state: Entity<KnownHostsPageState>,
    // 连接进度状态（按 tab_id 索引）
    pub connecting_progress: HashMap<String, Entity<ConnectingProgress>>,
    /// 上一次的 show_home 状态，用于检测视图切换
    last_show_home: bool,
}

impl HomePage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let sidebar_state = cx.new(|_| SidebarState {
            selected_menu: MenuType::Hosts,
        });

        let view_mode_state = cx.new(|_| ViewModeState::default());

        let dialog_state = cx.new(|_| ServerDialogState::default());
        let update_state = cx.new(|_| UpdateState::default());
        let settings_dialog_state =
            cx.new(|_| SettingsDialogState::with_updater(update_state.clone()));
        cx.observe(&update_state, |_, _, cx| cx.notify()).detach();
        let ui_state = storage::load_ui_state().unwrap_or_default();
        let session_state = cx.new(|_| SessionState::from_ui_state(&ui_state));
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
            update_state,
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
            .map(|group| ServerGroup {
                id: group.id.clone(),
                name: group.name.clone(),
                servers: config
                    .servers
                    .iter()
                    .filter(|s| s.group_id.as_deref() == Some(&group.id))
                    .map(server_view)
                    .collect(),
            })
            .collect();

        // 未分组的服务器放入 "未分组" 分组
        let ungrouped_servers: Vec<Server> = config
            .servers
            .iter()
            .filter(|s| s.group_id.is_none())
            .map(server_view)
            .collect();

        if !ungrouped_servers.is_empty() {
            server_groups.push(ServerGroup {
                id: ServerGroup::UNGROUPED_ID.to_string(),
                name: i18n::t(&lang, "server_list.ungrouped").to_string(),
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

    /// 从存储加载最近连接过的服务器，按连接时间倒序
    fn load_history() -> Vec<HistoryItem> {
        let config = storage::load_servers().unwrap_or_default();
        let mut history: Vec<HistoryItem> = config
            .servers
            .iter()
            .filter_map(|server| {
                let connected_at = parse_connected_at(server.last_connected_at.as_deref()?)?;
                Some(HistoryItem {
                    server_id: server.id.clone(),
                    name: server.label.clone(),
                    distro_icon: distribution_icon(
                        server.os_name.as_deref(),
                        &server.label,
                        server.description.as_deref().unwrap_or_default(),
                    ),
                    address: format!("{}@{}", server.username, server.host),
                    connected_at,
                })
            })
            .collect();

        // 按时间倒序排序
        history.sort_by(|a, b| b.connected_at.cmp(&a.connected_at));
        history.truncate(HISTORY_LIMIT);
        history
    }

    fn render_content(
        &self,
        selected_menu: MenuType,
        viewport: Size<Pixels>,
        fit: &TextFitter,
        cx: &Context<Self>,
    ) -> AnyElement {
        let lang = self
            .settings_dialog_state
            .read(cx)
            .settings
            .theme
            .language
            .clone();

        match selected_menu {
            MenuType::Hosts => render_hosts_content(
                HostsContent {
                    groups: &self.server_groups,
                    history: &self.history,
                    fit: fit.clone(),
                    viewport,
                    lang,
                    view_state: self.view_mode_state.clone(),
                    dialog_state: self.dialog_state.clone(),
                    session_state: self.session_state.clone(),
                },
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
                render_known_hosts_content(self.known_hosts_state.clone(), fit, cx)
                    .into_any_element()
            }
        }
    }

    /// 侧边栏导航下方显示的内容：服务器页的卡片视图显示分组（「最近连接」在内容区顶部），
    /// 其余情况显示「最近连接」
    fn sidebar_section(&self, selected_menu: MenuType, cx: &Context<Self>) -> SidebarSection<'_> {
        let view_state = self.view_mode_state.read(cx);
        let show_groups = selected_menu == MenuType::Hosts
            && view_state.mode == ViewMode::Card
            && !self.server_groups.is_empty();
        if show_groups {
            SidebarSection::Groups {
                groups: self
                    .server_groups
                    .iter()
                    .map(|group| GroupEntry {
                        id: group.id.clone(),
                        name: group.name.clone(),
                        server_count: group.servers.len(),
                    })
                    .collect(),
                selected: view_state.group_filter.clone(),
            }
        } else {
            SidebarSection::Recent(&self.history)
        }
    }

    /// 渲染主页视图（Sidebar + 顶部条 + 当前页面）
    fn render_home_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_menu = self.sidebar_state.read(cx).selected_menu;
        let dialog_visible = self.dialog_state.read(cx).visible;
        let settings_dialog_visible = self.settings_dialog_state.read(cx).visible;
        let dialog_state = self.dialog_state.clone();
        let settings_dialog_state = self.settings_dialog_state.clone();
        let lang = self
            .settings_dialog_state
            .read(cx)
            .settings
            .theme
            .language
            .clone();

        self.view_mode_state.update(cx, |state, cx| {
            state.ensure_search_input_created(window, cx);
        });

        let counts = NavCounts {
            servers: self.server_groups.iter().map(|g| g.servers.len()).sum(),
            snippets: self.snippets_state.read(cx).config.commands.len(),
            known_hosts: self.known_hosts_state.read(cx).hosts.len(),
        };
        let viewport = window.viewport_size();
        let fit = TextFitter::new(window, cx);

        div()
            .size_full()
            .bg(crate::theme::background_color(cx))
            .flex()
            .relative()
            // 左侧：Sidebar（从窗口顶部到底部）
            .child(render_sidebar(
                SidebarProps {
                    fit: fit.clone(),
                    state: self.sidebar_state.clone(),
                    selected_menu,
                    counts,
                    section: self.sidebar_section(selected_menu, cx),
                    view_state: self.view_mode_state.clone(),
                    settings_dialog_state: self.settings_dialog_state.clone(),
                    session_state: self.session_state.clone(),
                },
                cx,
            ))
            // 右侧：顶部条（有会话时显示标签页）+ 当前页面
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(render_home_topbar(self.session_state.clone(), &fit, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden() // 防止内容溢出
                            .child(self.render_content(selected_menu, viewport, &fit, cx)),
                    ),
            )
            // 服务器弹窗
            .children(if dialog_visible {
                self.dialog_state.update(cx, |state, cx| {
                    state.ensure_inputs_created(&lang, window, cx);
                });
                Some(render_server_dialog_overlay(
                    dialog_state,
                    viewport,
                    &fit,
                    cx,
                ))
            } else {
                None
            })
            // 删除服务器的确认弹窗
            .children(render_delete_confirm(
                &self.view_mode_state,
                &self.dialog_state,
                &lang,
                cx,
            ))
            // 设置弹窗
            .children(if settings_dialog_visible {
                self.settings_dialog_state.update(cx, |state, cx| {
                    state.ensure_inputs_created(window, cx);
                });
                Some(render_settings_dialog_overlay(
                    settings_dialog_state,
                    viewport,
                    cx,
                ))
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
                        state.ensure_inputs_created(&lang, window, cx);
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
        let lang = self
            .settings_dialog_state
            .read(cx)
            .settings
            .theme
            .language
            .clone();
        let state = session_state.read(cx);
        // 会话视图各栏的实际宽度：标题栏品牌区和会话布局共用
        let columns = state.session_columns(window.viewport_size().width);

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

                    // 确保 AI 对话输入框已创建
                    let tab_id_for_ai = tab.id.clone();
                    session_state.update(cx, |state, cx| {
                        state.ensure_ai_chat_input(&tab_id_for_ai, window, cx);
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

                        // 提取当前路径和加载状态（避免借用冲突）
                        let current_path = sftp_state.map(|s| s.current_path.clone());
                        let loading = sftp_state.is_some_and(|s| s.loading);

                        // 确保 PathBarState 创建并同步路径
                        let entity = cx.entity().clone();
                        let path_bar =
                            state.ensure_sftp_path_bar_state(&tab_id_for_sftp, entity, window, cx);
                        // 同步当前路径和加载状态到 PathBarState
                        path_bar.update(cx, |pb, cx| {
                            if let Some(path) = current_path {
                                pb.set_path(&path, window, cx);
                            }
                            pb.set_loading(loading, cx);
                        });
                    });

                    // 确保 SFTP 新建文件夹对话框输入框已创建
                    let new_folder_dialog = session_state.read(cx).sftp_new_folder_dialog.clone();
                    if let Some(dialog) = new_folder_dialog {
                        let is_open = dialog.read(cx).is_open;
                        if is_open {
                            dialog.update(cx, |ds, cx| {
                                ds.ensure_input_created(&lang, window, cx);
                            });
                        }
                    }

                    // 确保 SFTP 新建文件对话框输入框已创建
                    let new_file_dialog = session_state.read(cx).sftp_new_file_dialog.clone();
                    if let Some(dialog) = new_file_dialog {
                        let is_open = dialog.read(cx).is_open;
                        if is_open {
                            dialog.update(cx, |ds, cx| {
                                ds.ensure_input_created(&lang, window, cx);
                            });
                        }
                    }

                    // 确保 SFTP 内置编辑器状态已初始化并同步重载内容
                    let tab_id_for_editor = tab.id.clone();
                    session_state.update(cx, |state, cx| {
                        state.ensure_sftp_editor_states(&tab_id_for_editor, window, cx);
                    });
                    let latest_tab = session_state
                        .read(cx)
                        .active_tab()
                        .cloned()
                        .unwrap_or(tab);

                    render_session_layout(
                        &latest_tab,
                        columns,
                        session_state.clone(),
                        &lang,
                        window,
                        cx,
                    )
                    .into_any_element()
                }
                // 断开、重连中继续显示会话布局，终端区域会显示断开提示或重连进度
                SessionStatus::Disconnected | SessionStatus::Reconnecting { .. } => {
                    let tab_id_for_editor = tab.id.clone();
                    session_state.update(cx, |state, cx| {
                        state.ensure_sftp_editor_states(&tab_id_for_editor, window, cx);
                    });
                    let latest_tab = session_state
                        .read(cx)
                        .active_tab()
                        .cloned()
                        .unwrap_or(tab);

                    render_session_layout(
                        &latest_tab,
                        columns,
                        session_state.clone(),
                        &lang,
                        window,
                        cx,
                    )
                    .into_any_element()
                }
                SessionStatus::Error(_) => {
                    // 错误状态使用连接页面显示
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
                    // Home 按钮区域（与左列同宽）
                    .child(render_home_button(columns.left, session_state.clone(), cx))
                    // 会话标题栏（带标签页）
                    .child(render_session_titlebar(
                        session_state,
                        &TextFitter::new(window, cx),
                        cx,
                    )),
            )
            // 第二行：会话内容区域
            .child(div().flex_1().w_full().min_h(px(0.)).child(content))
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 首次渲染时按配置安排启动后的后台更新检查
        if !self.update_state.read(cx).startup_checked {
            UpdateState::schedule_startup_check(
                self.update_state.clone(),
                window.window_handle(),
                cx,
            );
        }

        // 统一的服务器列表刷新逻辑
        let show_home = self.session_state.read(cx).show_home;
        let needs_refresh_from_dialog = self.dialog_state.read(cx).needs_refresh;

        // 刷新条件：1) 从会话视图切换到主页视图  2) 对话框保存后需要刷新
        if (show_home && !self.last_show_home) || needs_refresh_from_dialog {
            self.reload_servers();
            // 分组可能随服务器的增删而消失，丢弃它们的折叠和筛选状态
            self.view_mode_state.update(cx, |state, _| {
                state.retain_groups(&self.server_groups);
            });
            // 连接新服务器时会记录主机密钥，侧边栏的数量和已知主机页要跟着更新
            self.known_hosts_state
                .update(cx, |state, _| state.refresh());
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
        // （通知浮层由 gpui-kit 的 Root 统一挂载在窗口最顶层，位置已在 theme::init 中设为 TopCenter）
        if show_home || !has_sessions {
            self.render_home_view(window, cx).into_any_element()
        } else {
            self.render_session_view(window, cx).into_any_element()
        }
    }
}
