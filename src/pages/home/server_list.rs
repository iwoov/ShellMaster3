// 服务器列表组件

use gpui::*;

use crate::components::common::icon::render_icon;
use crate::components::common::server_dialog::ServerDialogState;
use crate::constants::icons;
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
) -> impl IntoElement {
    let dialog_state_for_list = dialog_state.clone();
    let dialog_state_for_card = dialog_state.clone();

    div()
        .flex_1()
        .h_full()
        .overflow_hidden() // 防止内容溢出
        .bg(rgb(0xffffff))
        .flex()
        .flex_col()
        .child(
            div()
                .flex_shrink_0() // 不压缩
                .p_6()
                .pb_4()
                .child(render_toolbar(view_mode, view_state, dialog_state)),
        )
        .child(
            // 服务器列表/卡片（可滚动）
            div()
                .id("server-list-scroll")
                .flex_1()
                .overflow_y_scroll() // 垂直滚动
                .px_6()
                .pb_6()
                .child(match view_mode {
                    ViewMode::List => {
                        render_list_view(server_groups, dialog_state_for_list).into_any_element()
                    }
                    ViewMode::Card => {
                        render_card_view(server_groups, dialog_state_for_card).into_any_element()
                    }
                }),
        )
}

/// 渲染工具栏
fn render_toolbar(
    view_mode: ViewMode,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
) -> impl IntoElement {
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
                .bg(rgb(0x3b82f6))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x2563eb)))
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
                        .text_color(rgb(0xffffff))
                        .child("添加服务器"),
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
                            rgb(0x3b82f6)
                        } else {
                            rgb(0xf3f4f6)
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| {
                            s.bg(if view_mode == ViewMode::Card {
                                rgb(0x2563eb)
                            } else {
                                rgb(0xe5e7eb)
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
                                rgb(0xffffff).into()
                            } else {
                                rgb(0x6b7280).into()
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
                            rgb(0x3b82f6)
                        } else {
                            rgb(0xf3f4f6)
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| {
                            s.bg(if view_mode == ViewMode::List {
                                rgb(0x2563eb)
                            } else {
                                rgb(0xe5e7eb)
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
                                rgb(0xffffff).into()
                            } else {
                                rgb(0x6b7280).into()
                            },
                        )),
                ),
        )
}

/// 渲染列表视图
fn render_list_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            render_server_group(group, state)
        }))
}

/// 渲染卡片视图
fn render_card_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            render_card_group(group, state)
        }))
}

/// 渲染卡片模式的服务器组
fn render_card_group(
    group: ServerGroup,
    dialog_state: Entity<ServerDialogState>,
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
                .child(div().w_1().h_5().bg(rgb(0x3b82f6)).rounded_sm())
                .child(render_icon(group.icon_path, rgb(0x6b7280).into()))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x1f2937))
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
                    render_server_card(server, state)
                })),
        )
}

/// 渲染服务器卡片
fn render_server_card(server: Server, dialog_state: Entity<ServerDialogState>) -> impl IntoElement {
    let server_id = server.id.clone();
    let server_id_for_edit = server_id.clone();
    let server_id_for_delete = server_id.clone();
    let dialog_for_edit = dialog_state.clone();
    let dialog_for_delete = dialog_state;

    div()
        .id(SharedString::from(format!("card-{}", server_id)))
        .w(px(220.))
        .bg(rgb(0xffffff))
        .rounded_lg()
        .border_1()
        .border_color(rgb(0xe5e7eb))
        .p_4()
        .cursor_pointer()
        .hover(|s| s.border_color(rgb(0x3b82f6)).shadow_md())
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
                        .bg(rgb(0xeff6ff))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::TERMINAL, rgb(0x3b82f6).into())),
                )
                .child(
                    div().flex_1().overflow_hidden().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x1f2937))
                            .overflow_hidden()
                            .child(server.name.clone()),
                    ),
                ),
        )
        .child(
            // 中部：主机信息
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_xs().text_color(rgb(0x9ca3af)).child("HOST"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x6b7280))
                                .child(server.host.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_xs().text_color(rgb(0x9ca3af)).child("PORT"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x6b7280))
                                .child(server.port.to_string()),
                        ),
                ),
        )
        .child(
            // 底部：最近连接时间
            div()
                .pt_2()
                .border_t_1()
                .border_color(rgb(0xf3f4f6))
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
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
                                .hover(|s| s.bg(rgb(0xf3f4f6)).rounded_md())
                                .p_1()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    dialog_for_edit.update(cx, |s, _| {
                                        s.open_edit(server_id_for_edit.clone());
                                    });
                                })
                                .child(render_icon(icons::EDIT, rgb(0x9ca3af).into())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "card-delete-{}",
                                    server_id_for_delete.clone()
                                )))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xfee2e2)).rounded_md())
                                .p_1()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    if let Err(e) = storage::delete_server(&server_id_for_delete) {
                                        eprintln!("删除服务器失败: {}", e);
                                    }
                                    dialog_for_delete.update(cx, |s, _| {
                                        s.needs_refresh = true;
                                    });
                                })
                                .child(render_icon(icons::TRASH, rgb(0xef4444).into())),
                        ),
                ),
        )
}

/// 渲染服务器组（表格）
fn render_server_group(
    group: ServerGroup,
    dialog_state: Entity<ServerDialogState>,
) -> impl IntoElement {
    let servers_owned = group.servers.clone();
    div()
        .bg(rgb(0xffffff))
        .rounded_lg()
        .border_1()
        .border_color(rgb(0xe5e7eb))
        .overflow_hidden()
        .child(
            // 组标题
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w_1().h_5().bg(rgb(0x3b82f6)).rounded_sm())
                .child(render_icon(group.icon_path, rgb(0x6b7280).into()))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x1f2937))
                        .child(group.name.clone()),
                ),
        )
        .child(
            // 表格头
            div()
                .px_4()
                .py_2()
                .bg(rgb(0xf9fafb))
                .border_t_1()
                .border_b_1()
                .border_color(rgb(0xe5e7eb))
                .flex()
                .child(
                    div()
                        .w(px(180.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("服务器"),
                )
                .child(
                    div()
                        .w(px(140.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("主机"),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("端口"),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("描述"),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("账号"),
                )
                .child(
                    div()
                        .w(px(100.))
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("最近连接"),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child("操作"),
                ),
        )
        .child(
            // 表格内容
            div()
                .flex()
                .flex_col()
                .children(servers_owned.into_iter().map(move |server| {
                    let state = dialog_state.clone();
                    render_server_row(server, state)
                })),
        )
}

/// 渲染服务器行
fn render_server_row(server: Server, dialog_state: Entity<ServerDialogState>) -> impl IntoElement {
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
        .border_color(rgb(0xf3f4f6))
        .flex()
        .items_center()
        .hover(|s| s.bg(rgb(0xf9fafb)))
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
                        .bg(rgb(0xeff6ff))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::TERMINAL, rgb(0x3b82f6).into())),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x1f2937))
                        .child(server.name.clone()),
                ),
        )
        .child(
            div()
                .w(px(140.))
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(server.host.clone()),
        )
        .child(
            div()
                .w(px(80.))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .cursor_pointer()
                        .child(render_icon(icons::COPY, rgb(0x9ca3af).into())),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x6b7280))
                        .child(server.port.to_string()),
                ),
        )
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(rgb(0x9ca3af))
                .child(server.description.clone()),
        )
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(server.account.clone()),
        )
        .child(
            div()
                .w(px(100.))
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(server.last_connected.clone()),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .gap_3()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-edit-{}",
                            server_id_for_edit.clone()
                        )))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0xf3f4f6)).rounded_md())
                        .p_1()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            dialog_for_edit.update(cx, |s, _| {
                                s.open_edit(server_id_for_edit.clone());
                            });
                        })
                        .child(render_icon(icons::EDIT, rgb(0x3b82f6).into())),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-delete-{}",
                            server_id_for_delete.clone()
                        )))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0xfee2e2)).rounded_md())
                        .p_1()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            if let Err(e) = storage::delete_server(&server_id_for_delete) {
                                eprintln!("删除服务器失败: {}", e);
                            }
                            dialog_for_delete.update(cx, |s, _| {
                                s.needs_refresh = true;
                            });
                        })
                        .child(render_icon(icons::TRASH, rgb(0xef4444).into())),
                ),
        )
}

/// 渲染占位内容
pub fn render_placeholder(title: &str, description: &str) -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        .bg(rgb(0xffffff))
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_4()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x9ca3af))
                .child(title.to_string()),
        )
        .child(
            div()
                .text_base()
                .text_color(rgb(0xd1d5db))
                .child(description.to_string()),
        )
        .child(
            div()
                .mt_4()
                .text_sm()
                .text_color(rgb(0x9ca3af))
                .child("等待开发..."),
        )
}
