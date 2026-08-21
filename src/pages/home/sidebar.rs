// 主页左侧菜单栏

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::{render_colored_icon, render_icon};
use crate::components::common::settings_dialog::SettingsDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::HistoryItem;
use crate::state::SessionState;

/// 菜单类型
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuType {
    Hosts,
    Monitor,
    Snippets,
    KnownHosts,
}

impl MenuType {
    pub fn id(&self) -> &'static str {
        match self {
            MenuType::Hosts => "hosts",
            MenuType::Monitor => "monitor",
            MenuType::Snippets => "snippets",
            MenuType::KnownHosts => "known_hosts",
        }
    }

    pub fn label_key(&self) -> &'static str {
        match self {
            MenuType::Hosts => "sidebar.hosts",
            MenuType::Monitor => "sidebar.monitor",
            MenuType::Snippets => "sidebar.snippets",
            MenuType::KnownHosts => "sidebar.known_hosts",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            MenuType::Hosts => icons::SERVER,
            MenuType::Monitor => icons::MONITOR,
            MenuType::Snippets => icons::CODE,
            MenuType::KnownHosts => icons::FINGERPRINT,
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
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let menus = [
        MenuType::Hosts,
        MenuType::Monitor,
        MenuType::Snippets,
        MenuType::KnownHosts,
    ];

    let lang = &settings_dialog_state.read(cx).settings.theme.language;

    let sidebar_bg = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;
    let hover_bg = cx.theme().secondary_hover;
    let text_color = cx.theme().foreground;
    let muted_text = cx.theme().muted_foreground;
    let icon_color = cx.theme().muted_foreground;

    div()
        .w(px(220.))
        .h_full()
        .bg(sidebar_bg)
        .border_r_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .child(
            div()
                .px_3()
                .pt_5()
                .pb_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .px_2()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted_text)
                        .child(i18n::t(lang, "sidebar.workspace")),
                )
                .children(
                    menus
                        .into_iter()
                        .map(|menu| render_menu_item(menu, selected_menu, state.clone(), lang, cx)),
                ),
        )
        .child(div().mx_4().h(px(1.)).bg(border_color))
        .child(
            div()
                .px_5()
                .pb_2()
                .pt_4()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted_text)
                        .child(i18n::t(lang, "sidebar.history")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted_text.opacity(0.75))
                        .child(history.len().min(6).to_string()),
                ),
        )
        .child(
            div()
                .id("sidebar-history-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px_3()
                .pb_3()
                .flex()
                .flex_col()
                .gap_1()
                .children(history.iter().take(6).cloned().map(|item| {
                    let session_for_click = session_state.clone();
                    let server_id = item.server_id.clone();
                    let server_name = item.name.clone();
                    div()
                        .id(SharedString::from(format!("recent-server-{}", server_id)))
                        .px_2()
                        .py_2()
                        .rounded_lg()
                        .hover(move |s| s.bg(hover_bg))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_3()
                        .on_click(move |_, _, cx| {
                            session_for_click.update(cx, |state, cx| {
                                state.add_tab(server_id.clone(), server_name.clone());
                                state.ensure_monitor_detail_dialog(cx);
                            });
                        })
                        .children(item.distro_icon.map(|icon| {
                            div()
                                .w_8()
                                .h_8()
                                .rounded_md()
                                .bg(cx.theme().primary.opacity(0.1))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(render_colored_icon(icon, 20.))
                        }))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_color)
                                        .overflow_hidden()
                                        .child(item.name),
                                )
                                .child(div().text_xs().text_color(muted_text).child(item.time)),
                        )
                }))
                .children(history.is_empty().then(|| {
                    div()
                        .px_2()
                        .py_4()
                        .text_xs()
                        .text_color(muted_text)
                        .child(i18n::t(lang, "sidebar.no_history"))
                })),
        )
        .child(
            div().p_3().border_t_1().border_color(border_color).child(
                div()
                    .id("settings-btn")
                    .h_10()
                    .px_3()
                    .rounded_lg()
                    .hover(move |s| s.bg(hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_3()
                    .on_click(move |_, _, cx| {
                        settings_dialog_state.update(cx, |s, _| s.open());
                    })
                    .child(render_icon(icons::SETTINGS, icon_color.into()))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(text_color)
                            .child(i18n::t(lang, "sidebar.settings")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted_text)
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            ),
        )
}

/// 渲染菜单项
fn render_menu_item(
    menu: MenuType,
    selected_menu: MenuType,
    state: Entity<SidebarState>,
    lang: &Language,
    cx: &App,
) -> impl IntoElement {
    let selected = selected_menu == menu;

    let sidebar_bg = crate::theme::sidebar_color(cx);
    let hover_bg = cx.theme().muted;
    let fg_normal = cx.theme().foreground;
    let fg_muted = cx.theme().muted_foreground;

    let bg_color = if selected {
        cx.theme().primary.opacity(0.12)
    } else {
        sidebar_bg
    };
    let text_color = if selected { fg_normal } else { fg_muted };
    let icon_color = if selected {
        cx.theme().primary
    } else {
        fg_muted
    };

    div()
        .id(menu.id())
        .h_10()
        .px_2()
        .rounded_lg()
        .bg(bg_color)
        .hover(move |s| if selected { s } else { s.bg(hover_bg) })
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_3()
        .on_click(move |_event, _window, cx| {
            cx.update_entity(&state, |s, cx| {
                s.selected_menu = menu;
                cx.notify();
            });
        })
        .child(div().w_1().h_5().rounded_full().bg(if selected {
            cx.theme().primary
        } else {
            sidebar_bg
        }))
        .child(render_icon(menu.icon(), icon_color.into()))
        .child(
            div()
                .text_sm()
                .text_color(text_color)
                .child(i18n::t(lang, menu.label_key())),
        )
}
