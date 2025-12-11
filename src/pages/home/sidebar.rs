// 主页左侧菜单栏

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::components::common::settings_dialog::SettingsDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::HistoryItem;

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
            MenuType::KnownHosts => icons::USER,
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
    let hover_bg = cx.theme().muted;
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
            // 菜单项
            div().p_2().flex().flex_col().gap_1().children(
                menus
                    .into_iter()
                    .map(|menu| render_menu_item(menu, selected_menu, state.clone(), lang, cx)),
            ),
        )
        // 分割线
        .child(div().mx_4().my_1().h(px(1.)).bg(cx.theme().border))
        // 历史记录标题
        .child(
            div().px_4().pb_1().pt_2().child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(muted_text)
                    .child(i18n::t(lang, "sidebar.history")),
            ),
        )
        .child(
            // 历史记录内容
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
                        .hover(move |s| s.bg(hover_bg))
                        .cursor_pointer()
                        .child(
                            div()
                                .text_sm()
                                .text_color(text_color)
                                .child(item.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted_text)
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
                    .hover(move |s| s.bg(hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_click(move |_, _, cx| {
                        settings_dialog_state.update(cx, |s, _| s.open());
                    })
                    .child(render_icon(icons::SETTINGS, icon_color.into()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_color)
                            .child(i18n::t(lang, "sidebar.settings")),
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

    // 使用主题的 accent 颜色作为选中状态
    let accent = cx.theme().accent;
    let accent_fg = cx.theme().accent_foreground;
    let sidebar_bg = crate::theme::sidebar_color(cx);
    let hover_bg = cx.theme().muted;
    let fg_normal = cx.theme().foreground;
    let fg_muted = cx.theme().muted_foreground;

    let bg_color = if selected { accent } else { sidebar_bg };
    let text_color = if selected { accent_fg } else { fg_normal };
    let icon_color = if selected { accent_fg } else { fg_muted };

    div()
        .id(menu.id())
        .px_3()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .hover(move |s| if selected { s } else { s.bg(hover_bg) })
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
        .child(
            div()
                .text_sm()
                .text_color(text_color)
                .child(i18n::t(lang, menu.label_key())),
        )
}
