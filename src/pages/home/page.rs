// HomePage 主页组件

use gpui::*;

use super::server_list::{render_hosts_content, render_placeholder, ViewMode, ViewModeState};
use super::sidebar::{render_sidebar, MenuType, SidebarState};
use super::titlebar::render_titlebar;
use crate::components::common::server_dialog::{render_server_dialog_overlay, ServerDialogState};
use crate::constants::icons;
use crate::models::{HistoryItem, Server, ServerGroup};

/// 主页状态
pub struct HomePage {
    pub server_groups: Vec<ServerGroup>,
    pub history: Vec<HistoryItem>,
    pub sidebar_state: Entity<SidebarState>,
    pub view_mode_state: Entity<ViewModeState>,
    pub dialog_state: Entity<ServerDialogState>,
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

        // 从存储加载服务器数据
        let config = crate::services::storage::load_servers().unwrap_or_default();

        // 将 ServerData 转换为视图用的 Server 结构
        let server_groups = config
            .groups
            .iter()
            .map(|group| {
                let group_servers: Vec<Server> = config
                    .servers
                    .iter()
                    .filter(|s| s.group_id.as_deref() == Some(&group.id))
                    .map(|s| Server {
                        name: s.label.clone(),
                        host: s.host.clone(),
                        port: s.port,
                        description: "-".into(),
                        account: s.username.clone(),
                        last_connected: s
                            .last_connected_at
                            .clone()
                            .unwrap_or_else(|| "从未".to_string()),
                    })
                    .collect();

                ServerGroup {
                    name: group.name.clone(),
                    icon_path: icons::SERVER,
                    servers: group_servers,
                }
            })
            .collect::<Vec<_>>();

        // 未分组的服务器放入 "默认" 分组
        let ungrouped_servers: Vec<Server> = config
            .servers
            .iter()
            .filter(|s| s.group_id.is_none())
            .map(|s| Server {
                name: s.label.clone(),
                host: s.host.clone(),
                port: s.port,
                description: "-".into(),
                account: s.username.clone(),
                last_connected: s
                    .last_connected_at
                    .clone()
                    .unwrap_or_else(|| "从未".to_string()),
            })
            .collect();

        let mut final_groups = server_groups;
        if !ungrouped_servers.is_empty() {
            final_groups.push(ServerGroup {
                name: "未分组".to_string(),
                icon_path: icons::SERVER,
                servers: ungrouped_servers,
            });
        }

        Self {
            server_groups: final_groups,
            history: vec![
                HistoryItem {
                    name: "Los Angles-DMIT".into(),
                    time: "24分钟前".into(),
                },
                HistoryItem {
                    name: "AAITR-NAT".into(),
                    time: "26分钟前".into(),
                },
            ],
            sidebar_state,
            view_mode_state,
            dialog_state,
        }
    }

    fn render_content(&self, selected_menu: MenuType, cx: &Context<Self>) -> AnyElement {
        let view_mode = self.view_mode_state.read(cx).mode;
        match selected_menu {
            MenuType::Hosts => render_hosts_content(
                &self.server_groups,
                view_mode,
                self.view_mode_state.clone(),
                self.dialog_state.clone(),
            )
            .into_any_element(),
            MenuType::Snippets => render_placeholder("Snippets", "代码片段功能").into_any_element(),
            MenuType::KnownHosts => {
                render_placeholder("Known Hosts", "已知主机管理").into_any_element()
            }
            MenuType::History => render_placeholder("History", "连接历史记录").into_any_element(),
        }
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = self.history.clone();
        let sidebar_state = self.sidebar_state.clone();
        let selected_menu = self.sidebar_state.read(cx).selected_menu;
        let dialog_visible = self.dialog_state.read(cx).visible;
        let dialog_state = self.dialog_state.clone();

        // 新布局：sidebar 在左侧从顶到底，右侧是 titlebar + content
        div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
            .relative() // 让弹窗可以绝对定位
            // 左侧 sidebar（从顶到底）
            .child(render_sidebar(sidebar_state, selected_menu, &history))
            // 右侧区域（titlebar + content）
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(render_titlebar())
                    .child(self.render_content(selected_menu, cx)),
            )
            // 条件渲染弹窗
            .children(if dialog_visible {
                // 确保输入框已创建
                self.dialog_state.update(cx, |state, cx| {
                    state.ensure_inputs_created(window, cx);
                });
                Some(render_server_dialog_overlay(dialog_state, cx))
            } else {
                None
            })
    }
}
