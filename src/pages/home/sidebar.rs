// 主页左侧菜单栏

use gpui::*;

use crate::components::common::icon::render_icon;
use crate::components::common::settings_dialog::SettingsDialogState;
use crate::constants::icons;
use crate::models::HistoryItem;

/// 菜单类型
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuType {
    Hosts,
    Snippets,
    KnownHosts,
    History,
}

impl MenuType {
    pub fn id(&self) -> &'static str {
        match self {
            MenuType::Hosts => "hosts",
            MenuType::Snippets => "snippets",
            MenuType::KnownHosts => "known_hosts",
            MenuType::History => "history",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            MenuType::Hosts => "Hosts",
            MenuType::Snippets => "Snippets",
            MenuType::KnownHosts => "Known Hosts",
            MenuType::History => "History",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            MenuType::Hosts => icons::SERVER,
            MenuType::Snippets => icons::CODE,
            MenuType::KnownHosts => icons::USER,
            MenuType::History => icons::HISTORY,
        }
    }
}

/// 侧边栏状态
pub struct SidebarState {
    pub selected_menu: MenuType,
}

/// 渲染侧边栏（从窗口顶部到底部）
pub fn render_sidebar(
    state: Entity<SidebarState>,
    selected_menu: MenuType,
    history: &[HistoryItem],
    settings_dialog_state: Entity<SettingsDialogState>,
) -> impl IntoElement {
    let menus = [
        MenuType::Hosts,
        MenuType::Snippets,
        MenuType::KnownHosts,
        MenuType::History,
    ];

    div()
        .w(px(220.))
        .h_full()
        .bg(rgb(0xf8f9fa))
        .border_r_1()
        .border_color(rgb(0xe5e7eb))
        .flex()
        .flex_col()
        .child(
            // 顶部空白区域（为 macOS 红绿灯留空间）
            div().h(px(52.)),
        )
        .child(
            // 菜单项
            div().p_2().flex().flex_col().gap_1().children(
                menus
                    .into_iter()
                    .map(|menu| render_menu_item(menu, selected_menu, state.clone())),
            ),
        )
        .child(
            // 历史记录
            div()
                .flex_1()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .children(history.iter().map(|item| {
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .hover(|s| s.bg(rgb(0xe5e7eb)))
                        .cursor_pointer()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x374151))
                                .child(item.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x9ca3af))
                                .child(item.time.clone()),
                        )
                })),
        )
        .child(
            // 底部设置按钮
            div().p_2().child(
                div()
                    .id("settings-btn")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .hover(|s| s.bg(rgb(0xe5e7eb)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_click(move |_, _, cx| {
                        settings_dialog_state.update(cx, |s, _| s.open());
                    })
                    .child(render_icon(icons::SETTINGS, rgb(0x6b7280).into()))
                    .child(div().text_sm().text_color(rgb(0x374151)).child("Settings")),
            ),
        )
}

/// 渲染菜单项
fn render_menu_item(
    menu: MenuType,
    selected_menu: MenuType,
    state: Entity<SidebarState>,
) -> impl IntoElement {
    let selected = selected_menu == menu;
    let bg_color = if selected {
        rgb(0xdbeafe)
    } else {
        rgb(0xf8f9fa)
    };
    let hover_bg = if selected {
        rgb(0xdbeafe)
    } else {
        rgb(0xe5e7eb)
    };
    let icon_color = if selected {
        rgb(0x2563eb)
    } else {
        rgb(0x6b7280)
    };
    let text_color = if selected {
        rgb(0x2563eb)
    } else {
        rgb(0x374151)
    };

    div()
        .id(menu.id())
        .px_3()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .hover(|s| s.bg(hover_bg))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .on_click(move |_event, _window, cx| {
            cx.update_entity(&state, |s, cx| {
                s.selected_menu = menu;
                cx.notify();
            });
        })
        .child(render_icon(menu.icon(), icon_color.into()))
        .child(div().text_sm().text_color(text_color).child(menu.label()))
}
