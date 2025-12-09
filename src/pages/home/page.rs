// HomePage 主页组件

use gpui::*;

use super::server_list::{render_hosts_content, render_placeholder, ViewMode, ViewModeState};
use super::sidebar::{render_sidebar, MenuType, SidebarState};
use super::titlebar::render_titlebar;
use crate::constants::icons;
use crate::models::{HistoryItem, Server, ServerGroup};

/// 主页状态
pub struct HomePage {
    pub server_groups: Vec<ServerGroup>,
    pub history: Vec<HistoryItem>,
    pub sidebar_state: Entity<SidebarState>,
    pub view_mode_state: Entity<ViewModeState>,
}

impl HomePage {
    pub fn new(cx: &mut App) -> Self {
        let sidebar_state = cx.new(|_| SidebarState {
            selected_menu: MenuType::Hosts,
        });

        let view_mode_state = cx.new(|_| ViewModeState {
            mode: ViewMode::List,
        });

        Self {
            server_groups: vec![
                ServerGroup {
                    name: "JPN".into(),
                    icon_path: icons::SERVER,
                    servers: vec![
                        Server {
                            name: "ByteVirt-IJJ".into(),
                            host: "31.57.172.165".into(),
                            port: 22,
                            description: "-".into(),
                            account: "root".into(),
                            last_connected: "12月7日".into(),
                        },
                        Server {
                            name: "拼好鸡-SoftBank".into(),
                            host: "23.176.40.184".into(),
                            port: 44429,
                            description: "-".into(),
                            account: "root".into(),
                            last_connected: "17小时前".into(),
                        },
                    ],
                },
                ServerGroup {
                    name: "Home".into(),
                    icon_path: icons::HOME,
                    servers: vec![Server {
                        name: "Home-WSL".into(),
                        host: "10.0.0.7".into(),
                        port: 8023,
                        description: "-".into(),
                        account: "root".into(),
                        last_connected: "12月7日".into(),
                    }],
                },
                ServerGroup {
                    name: "CN".into(),
                    icon_path: icons::SERVER,
                    servers: vec![Server {
                        name: "NingBo-ChinaTel".into(),
                        host: "110.42.98.184".into(),
                        port: 9822,
                        description: "-".into(),
                        account: "root".into(),
                        last_connected: "12月8日".into(),
                    }],
                },
            ],
            history: vec![
                HistoryItem {
                    name: "Los Angles-DMIT".into(),
                    time: "24分钟前".into(),
                },
                HistoryItem {
                    name: "AAITR-NAT".into(),
                    time: "26分钟前".into(),
                },
                HistoryItem {
                    name: "HongHong-Cera".into(),
                    time: "12小时前".into(),
                },
                HistoryItem {
                    name: "Singapore-OrangeVPS".into(),
                    time: "17小时前".into(),
                },
                HistoryItem {
                    name: "拼好鸡-SoftBank".into(),
                    time: "17小时前".into(),
                },
            ],
            sidebar_state,
            view_mode_state,
        }
    }

    fn render_content(&self, selected_menu: MenuType, cx: &Context<Self>) -> AnyElement {
        let view_mode = self.view_mode_state.read(cx).mode;
        match selected_menu {
            MenuType::Hosts => {
                render_hosts_content(&self.server_groups, view_mode, self.view_mode_state.clone())
                    .into_any_element()
            }
            MenuType::Snippets => render_placeholder("Snippets", "代码片段功能").into_any_element(),
            MenuType::KnownHosts => {
                render_placeholder("Known Hosts", "已知主机管理").into_any_element()
            }
            MenuType::History => render_placeholder("History", "连接历史记录").into_any_element(),
        }
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = self.history.clone();
        let sidebar_state = self.sidebar_state.clone();
        let selected_menu = self.sidebar_state.read(cx).selected_menu;

        // 新布局：sidebar 在左侧从顶到底，右侧是 titlebar + content
        div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
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
    }
}
