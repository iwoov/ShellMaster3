// 服务器列表组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::components::common::server_dialog::ServerDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{Server, ServerGroup};
use crate::services::storage;

/// 视图模式
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Card,
}

/// 视图模式状态
pub struct ViewModeState {
    pub mode: ViewMode,
}

/// 渲染主机内容区域（包含工具栏和服务器列表）
pub fn render_hosts_content(
    server_groups: &[ServerGroup],
    view_mode: ViewMode,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    let dialog_state_for_list = dialog_state.clone();
    let dialog_state_for_card = dialog_state.clone();
    let dialog_state_for_empty = dialog_state.clone();

    // 检查是否有任何服务器
    let has_servers = server_groups.iter().any(|g| !g.servers.is_empty());

    let bg_color = crate::theme::background_color(cx);

    div()
        .flex_1()
        .h_full()
        .overflow_hidden() // 防止内容溢出
        .bg(bg_color)
        .flex()
        .flex_col()
        .child(if has_servers {
            // 有服务器时显示工具栏
            div()
                .flex_shrink_0() // 不压缩
                .p_6()
                .pb_4()
                .child(render_toolbar(view_mode, view_state, dialog_state, cx))
                .into_any_element()
        } else {
            // 没有服务器时不显示工具栏
            div().into_any_element()
        })
        .child(if has_servers {
            // 有服务器时显示服务器列表/卡片
            div()
                .id("server-list-scroll")
                .flex_1()
                .overflow_y_scroll() // 垂直滚动
                .px_6()
                .pb_6()
                .child(match view_mode {
                    ViewMode::List => render_list_view(server_groups, dialog_state_for_list, cx)
                        .into_any_element(),
                    ViewMode::Card => render_card_view(server_groups, dialog_state_for_card, cx)
                        .into_any_element(),
                })
                .into_any_element()
        } else {
            // 没有服务器时显示空状态
            render_empty_state(dialog_state_for_empty, cx).into_any_element()
        })
}

/// 渲染工具栏
fn render_toolbar(
    view_mode: ViewMode,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_for_card = view_state.clone();
    let state_for_list = view_state;

    div()
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .id("add-server-btn")
                .px_4()
                .py_2()
                .bg(cx.theme().primary)
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(cx.theme().primary_hover))
                .flex()
                .items_center()
                .gap_2()
                .on_click(move |_, _, cx| {
                    dialog_state.update(cx, |s, _| s.open_add());
                })
                .child(render_icon(icons::PLUS, rgb(0xffffff).into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().primary_foreground)
                        .child(i18n::t(&lang, "server_list.add_server")),
                ),
        )
        .child(
            div()
                .flex()
                .gap_1()
                .child(
                    // 卡片视图按钮
                    div()
                        .id("view-card-btn")
                        .w_9()
                        .h_9()
                        .rounded_md()
                        .bg(if view_mode == ViewMode::Card {
                            cx.theme().primary
                        } else {
                            cx.theme().secondary
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| {
                            s.bg(if view_mode == ViewMode::Card {
                                cx.theme().primary_hover
                            } else {
                                cx.theme().secondary_hover
                            })
                        })
                        .on_click(move |_, _, cx| {
                            cx.update_entity(&state_for_card, |s, cx| {
                                s.mode = ViewMode::Card;
                                cx.notify();
                            });
                        })
                        .child(render_icon(
                            icons::GRID,
                            if view_mode == ViewMode::Card {
                                cx.theme().primary_foreground.into()
                            } else {
                                cx.theme().muted_foreground.into()
                            },
                        )),
                )
                .child(
                    // 列表视图按钮
                    div()
                        .id("view-list-btn")
                        .w_9()
                        .h_9()
                        .rounded_md()
                        .bg(if view_mode == ViewMode::List {
                            cx.theme().primary
                        } else {
                            cx.theme().secondary
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| {
                            s.bg(if view_mode == ViewMode::List {
                                cx.theme().primary_hover
                            } else {
                                cx.theme().secondary_hover
                            })
                        })
                        .on_click(move |_, _, cx| {
                            cx.update_entity(&state_for_list, |s, cx| {
                                s.mode = ViewMode::List;
                                cx.notify();
                            });
                        })
                        .child(render_icon(
                            icons::LIST,
                            if view_mode == ViewMode::List {
                                cx.theme().primary_foreground.into()
                            } else {
                                cx.theme().muted_foreground.into()
                            },
                        )),
                ),
        )
}

/// 渲染列表视图
fn render_list_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        secondary_hover: cx.theme().secondary_hover,
        destructive: rgb(0xef4444).into(),
        header_bg: crate::theme::sidebar_color(cx),
    };

    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            render_server_group(group, state, colors)
        }))
}

/// 渲染卡片视图
#[derive(Clone, Copy)]
struct CardColors {
    bg: Hsla,
    border: Hsla,
    primary: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    secondary_hover: Hsla,
    destructive: Hsla,
    header_bg: Hsla,
}

fn render_card_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        secondary_hover: cx.theme().secondary_hover,
        destructive: rgb(0xef4444).into(),
        header_bg: crate::theme::sidebar_color(cx),
    };

    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            render_card_group(group, state, colors)
        }))
}

/// 渲染卡片模式的服务器组
fn render_card_group(
    group: ServerGroup,
    dialog_state: Entity<ServerDialogState>,
    colors: CardColors,
) -> impl IntoElement {
    let servers_owned = group.servers.clone();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            // 组标题
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w_1().h_5().bg(colors.primary).rounded_sm())
                .child(render_icon(group.icon_path, colors.muted_foreground.into()))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(group.name.clone()),
                ),
        )
        .child(
            // 卡片网格
            div()
                .flex()
                .flex_wrap()
                .gap_4()
                .children(servers_owned.into_iter().map(move |server| {
                    let state = dialog_state.clone();
                    render_server_card(server, state, colors)
                })),
        )
}

/// 渲染服务器卡片
fn render_server_card(
    server: Server,
    dialog_state: Entity<ServerDialogState>,
    colors: CardColors,
) -> impl IntoElement {
    let server_id = server.id.clone();
    let server_id_for_edit = server_id.clone();
    let server_id_for_delete = server_id.clone();
    let dialog_for_edit = dialog_state.clone();
    let dialog_for_delete = dialog_state;

    div()
        .id(SharedString::from(format!("card-{}", server_id)))
        .w(px(220.))
        .bg(colors.bg)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .p_4()
        .cursor_pointer()
        .hover(move |s| s.border_color(colors.primary).shadow_md())
        .flex()
        .flex_col()
        .gap_3()
        .child(
            // 顶部：图标和名称
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .w_10()
                        .h_10()
                        .rounded_lg()
                        .bg(colors.primary.opacity(0.1))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::TERMINAL, colors.primary.into())),
                )
                .child(
                    div().flex_1().overflow_hidden().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .overflow_hidden()
                            .child(server.name.clone()),
                    ),
                ),
        )
        .child({
            // 中部：主机信息
            let host_for_copy = server.host.clone();
            let server_id_for_copy = server_id.clone();
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(colors.muted_foreground)
                                        .child("HOST"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(colors.muted_foreground)
                                        .child(server.host.clone()),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "card-copy-host-{}",
                                    server_id_for_copy
                                )))
                                .cursor_pointer()
                                .p_1()
                                .rounded_sm()
                                .hover(move |s| s.bg(colors.secondary_hover))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        host_for_copy.clone(),
                                    ));
                                })
                                .child(render_icon(icons::COPY, colors.muted_foreground.into())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("PORT"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(colors.muted_foreground)
                                .child(server.port.to_string()),
                        ),
                )
        })
        .child(
            // 底部：最近连接时间
            div()
                .pt_2()
                .border_t_1()
                .border_color(colors.border)
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(format!("{} · {}", server.account, server.last_connected)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "card-edit-{}",
                                    server_id_for_edit.clone()
                                )))
                                .cursor_pointer()
                                .hover(move |s| s.bg(colors.secondary_hover).rounded_md())
                                .p_1()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    dialog_for_edit.update(cx, |s, _| {
                                        s.open_edit(server_id_for_edit.clone());
                                    });
                                })
                                .child(render_icon(icons::EDIT, colors.muted_foreground.into())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "card-delete-{}",
                                    server_id_for_delete.clone()
                                )))
                                .cursor_pointer()
                                .hover(move |s| s.bg(colors.destructive.opacity(0.1)).rounded_md())
                                .p_1()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    if let Err(e) = storage::delete_server(&server_id_for_delete) {
                                        eprintln!("Failed to delete server: {}", e);
                                    }
                                    dialog_for_delete.update(cx, |s, _| {
                                        s.needs_refresh = true;
                                    });
                                })
                                .child(render_icon(icons::TRASH, colors.destructive.into())),
                        ),
                ),
        )
}

/// 渲染服务器组（表格）
fn render_server_group(
    group: ServerGroup,
    dialog_state: Entity<ServerDialogState>,
    colors: CardColors,
) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let servers_owned = group.servers.clone();
    div()
        .bg(colors.bg)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .overflow_hidden()
        .child(
            // 组标题
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w_1().h_5().bg(colors.primary).rounded_sm())
                .child(render_icon(group.icon_path, colors.muted_foreground.into()))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(group.name.clone()),
                ),
        )
        .child(
            // 表格头
            div()
                .px_4()
                .py_2()
                .bg(colors.header_bg)
                .border_t_1()
                .border_b_1()
                .border_color(colors.border)
                .flex()
                .child(
                    div()
                        .w(px(180.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.server")),
                )
                .child(
                    div()
                        .w(px(170.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.host")),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.port")),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.description")),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.account")),
                )
                .child(
                    div()
                        .w(px(100.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.last_connected")),
                )
                .child(
                    div()
                        .w(px(70.))
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.actions")),
                ),
        )
        .child(
            // 表格内容
            div()
                .flex()
                .flex_col()
                .children(servers_owned.into_iter().map(move |server| {
                    let state = dialog_state.clone();
                    render_server_row(server, state, colors)
                })),
        )
}

/// 渲染服务器行
fn render_server_row(
    server: Server,
    dialog_state: Entity<ServerDialogState>,
    colors: CardColors,
) -> impl IntoElement {
    let server_id = server.id.clone();
    let server_id_for_edit = server_id.clone();
    let server_id_for_delete = server_id.clone();
    let dialog_for_edit = dialog_state.clone();
    let dialog_for_delete = dialog_state;

    div()
        .id(SharedString::from(format!("row-{}", server_id)))
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(colors.border)
        .flex()
        .items_center()
        .hover(move |s| s.bg(colors.header_bg))
        .child(
            div()
                .w(px(180.))
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .w_8()
                        .h_8()
                        .rounded_md()
                        .bg(colors.primary.opacity(0.1))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::TERMINAL, colors.primary.into())),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(colors.foreground)
                        .child(server.name.clone()),
                ),
        )
        .child({
            let host_for_copy = server.host.clone();
            div()
                .w(px(170.))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(colors.muted_foreground)
                        .child(server.host.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "copy-host-{}",
                            server_id.clone()
                        )))
                        .cursor_pointer()
                        .p_1()
                        .rounded_sm()
                        .hover(move |s| s.bg(colors.secondary_hover))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            cx.write_to_clipboard(ClipboardItem::new_string(host_for_copy.clone()));
                        })
                        .child(render_icon(icons::COPY, colors.muted_foreground.into())),
                )
        })
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(server.port.to_string()),
        )
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(server.description.clone()),
        )
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(server.account.clone()),
        )
        .child(
            div()
                .w(px(100.))
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(server.last_connected.clone()),
        )
        .child(
            div()
                .w(px(80.))
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-edit-{}",
                            server_id_for_edit.clone()
                        )))
                        .cursor_pointer()
                        .hover(move |s| s.bg(colors.secondary_hover).rounded_md())
                        .p_1()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            dialog_for_edit.update(cx, |s, _| {
                                s.open_edit(server_id_for_edit.clone());
                            });
                        })
                        .child(render_icon(icons::EDIT, colors.primary.into())),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-delete-{}",
                            server_id_for_delete.clone()
                        )))
                        .cursor_pointer()
                        .hover(move |s| s.bg(colors.destructive.opacity(0.1)).rounded_md())
                        .p_1()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            if let Err(e) = storage::delete_server(&server_id_for_delete) {
                                eprintln!("Failed to delete server: {}", e);
                            }
                            dialog_for_delete.update(cx, |s, _| {
                                s.needs_refresh = true;
                            });
                        })
                        .child(render_icon(icons::TRASH, colors.destructive.into())),
                ),
        )
}

/// 渲染空状态（没有服务器时显示）
fn render_empty_state(dialog_state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_6()
        .child(
            // 服务器图标
            div()
                .w_20()
                .h_20()
                .rounded_2xl()
                .bg(cx.theme().primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::SERVER, cx.theme().primary.into())),
        )
        .child(
            // 提示文字
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().muted_foreground)
                        .child(i18n::t(&lang, "server_list.empty_title")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(i18n::t(&lang, "server_list.empty_description")),
                ),
        )
        .child(
            // 添加服务器按钮
            div()
                .id("empty-add-server-btn")
                .px_6()
                .py_3()
                .bg(cx.theme().primary)
                .rounded_lg()
                .cursor_pointer()
                .hover(move |s| s.bg(cx.theme().primary_hover))
                .flex()
                .items_center()
                .gap_2()
                .on_click(move |_, _, cx| {
                    dialog_state.update(cx, |s, _| s.open_add());
                })
                .child(render_icon(icons::PLUS, rgb(0xffffff).into()))
                .child(
                    div()
                        .text_base()
                        .text_color(cx.theme().primary_foreground)
                        .child(i18n::t(&lang, "server_list.add_server")),
                ),
        )
}

/// 渲染占位内容
pub fn render_placeholder(title: &str, description: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        // .bg(rgb(0xffffff)) // Removed hardcoded white
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_4()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_base()
                .text_color(cx.theme().muted_foreground)
                .child(description.to_string()),
        )
        .child(
            div()
                .mt_4()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Coming soon..."),
        )
}
