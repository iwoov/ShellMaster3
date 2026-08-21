// 服务器列表组件

use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{ActiveTheme, InteractiveElementExt, Sizable};
use tracing::error;

use crate::components::common::icon::{
    distribution_icon as detect_distribution_icon, render_colored_icon, render_icon,
};
use crate::components::common::server_dialog::ServerDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{Server, ServerGroup};
use crate::services::storage;
use crate::state::SessionState;

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
    pub query: String,
    pub search_input: Option<Entity<InputState>>,
    pub copied_server_id: Option<String>,
    copy_feedback_token: u64,
}

impl Default for ViewModeState {
    fn default() -> Self {
        Self {
            mode: ViewMode::List,
            query: String::new(),
            search_input: None,
            copied_server_id: None,
            copy_feedback_token: 0,
        }
    }
}

impl ViewModeState {
    /// 搜索框依赖 Window 创建，首次进入主页时初始化并订阅实时输入。
    pub fn ensure_search_input_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_input.is_some() {
            return;
        }

        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);
        let placeholder = i18n::t(&lang, "server_list.search_placeholder");
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));

        cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = input.read(cx).value().trim().to_string();
                cx.notify();
            }
        })
        .detach();

        self.search_input = Some(input);
    }
}

/// 渲染主机内容区域（包含工具栏和服务器列表）
pub fn render_hosts_content(
    server_groups: &[ServerGroup],
    view_mode: ViewMode,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let total_servers = server_groups.iter().map(|g| g.servers.len()).sum::<usize>();
    let query = view_state.read(cx).query.clone();
    let filtered_groups = filter_server_groups(server_groups, &query);
    let filtered_servers = filtered_groups
        .iter()
        .map(|g| g.servers.len())
        .sum::<usize>();

    let dialog_state_for_list = dialog_state.clone();
    let dialog_state_for_card = dialog_state.clone();
    let dialog_state_for_empty = dialog_state.clone();
    let session_state_for_list = session_state.clone();
    let session_state_for_card = session_state;
    let view_state_for_toolbar = view_state.clone();
    let view_state_for_list = view_state.clone();
    let view_state_for_card = view_state;

    let has_servers = total_servers > 0;
    let has_matches = filtered_servers > 0;
    let bg_color = crate::theme::background_color(cx);

    div()
        .flex_1()
        .h_full()
        .overflow_hidden() // 防止内容溢出
        .bg(bg_color)
        .flex()
        .flex_col()
        .child(
            div()
                .flex_shrink_0()
                .px_6()
                .pt_5()
                .pb_4()
                .child(render_toolbar(
                    view_mode,
                    view_state_for_toolbar,
                    dialog_state,
                    cx,
                )),
        )
        .child(if has_servers {
            if has_matches {
                div()
                    .id("server-list-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_x_hidden()
                    .overflow_y_scrollbar()
                    .px_6()
                    .pb_6()
                    .child(match view_mode {
                        ViewMode::List => render_list_view(
                            &filtered_groups,
                            dialog_state_for_list,
                            session_state_for_list,
                            view_state_for_list,
                            cx,
                        )
                        .into_any_element(),
                        ViewMode::Card => render_card_view(
                            &filtered_groups,
                            dialog_state_for_card,
                            session_state_for_card,
                            view_state_for_card,
                            cx,
                        )
                        .into_any_element(),
                    })
                    .into_any_element()
            } else {
                render_search_empty(&query, cx).into_any_element()
            }
        } else {
            render_empty_state(dialog_state_for_empty, cx).into_any_element()
        })
}

fn filter_server_groups(server_groups: &[ServerGroup], query: &str) -> Vec<ServerGroup> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return server_groups.to_vec();
    }

    server_groups
        .iter()
        .filter_map(|group| {
            let group_matches = group.name.to_lowercase().contains(&query);
            let servers = group
                .servers
                .iter()
                .filter(|server| {
                    group_matches
                        || server.name.to_lowercase().contains(&query)
                        || server.host.to_lowercase().contains(&query)
                        || server.account.to_lowercase().contains(&query)
                        || server.description.to_lowercase().contains(&query)
                        || server.port.to_string().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>();

            (!servers.is_empty()).then(|| ServerGroup {
                name: group.name.clone(),
                icon_path: group.icon_path,
                servers,
            })
        })
        .collect()
}

/// 渲染工具栏
fn render_toolbar(
    view_mode: ViewMode,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_card = view_state.clone();
    let state_for_list = view_state.clone();
    let search_input = view_state.read(cx).search_input.clone();

    div()
        .flex()
        .flex_wrap()
        .justify_between()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .children(search_input.map(|input| {
                    div()
                        .w(px(340.))
                        .h_10()
                        .px_3()
                        .rounded_lg()
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(render_icon(
                            icons::SEARCH,
                            cx.theme().muted_foreground.into(),
                        ))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&input).appearance(false).small()),
                        )
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .h_10()
                        .p_1()
                        .rounded_lg()
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(render_view_button(
                            "view-list-btn",
                            icons::LIST,
                            view_mode == ViewMode::List,
                            state_for_list,
                            ViewMode::List,
                            cx,
                        ))
                        .child(render_view_button(
                            "view-card-btn",
                            icons::GRID,
                            view_mode == ViewMode::Card,
                            state_for_card,
                            ViewMode::Card,
                            cx,
                        )),
                )
                .child(
                    div()
                        .id("add-server-btn")
                        .w_10()
                        .h_10()
                        .bg(cx.theme().primary)
                        .rounded_lg()
                        .cursor_pointer()
                        .hover(move |s| s.bg(cx.theme().primary_hover))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(move |_, _, cx| {
                            dialog_state.update(cx, |s, _| s.open_add());
                        })
                        .child(render_icon(icons::PLUS, rgb(0xffffff).into())),
                ),
        )
}

fn render_view_button(
    id: &'static str,
    icon: &'static str,
    selected: bool,
    state: Entity<ViewModeState>,
    mode: ViewMode,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .w_8()
        .h_8()
        .rounded_md()
        .bg(if selected {
            cx.theme().secondary
        } else {
            cx.theme().popover
        })
        .cursor_pointer()
        .hover(move |s| s.bg(cx.theme().secondary_hover))
        .flex()
        .items_center()
        .justify_center()
        .on_click(move |_, _, cx| {
            state.update(cx, |s, cx| {
                s.mode = mode;
                cx.notify();
            });
        })
        .child(render_icon(
            icon,
            if selected {
                cx.theme().foreground.into()
            } else {
                cx.theme().muted_foreground.into()
            },
        ))
}

/// 渲染列表视图
fn render_list_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    cx: &App,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();
    let copied_server_id = view_state.read(cx).copied_server_id.clone();

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        secondary_hover: cx.theme().secondary_hover,
        destructive: rgb(0xef4444).into(),
        success: rgb(0x22c55e).into(),
        header_bg: crate::theme::sidebar_color(cx),
    };

    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            let sess = session_state.clone();
            render_server_group(
                group,
                state,
                sess,
                view_state.clone(),
                copied_server_id.clone(),
                colors,
            )
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
    success: Hsla,
    header_bg: Hsla,
}

fn render_card_view(
    server_groups: &[ServerGroup],
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    cx: &App,
) -> impl IntoElement {
    let groups_owned: Vec<ServerGroup> = server_groups.to_vec();
    let copied_server_id = view_state.read(cx).copied_server_id.clone();

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        secondary_hover: cx.theme().secondary_hover,
        destructive: rgb(0xef4444).into(),
        success: rgb(0x22c55e).into(),
        header_bg: crate::theme::sidebar_color(cx),
    };

    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_6()
        .children(groups_owned.into_iter().map(move |group| {
            let state = dialog_state.clone();
            let sess = session_state.clone();
            render_card_group(
                group,
                state,
                sess,
                view_state.clone(),
                copied_server_id.clone(),
                colors,
            )
        }))
}

/// 渲染卡片模式的服务器组
fn render_card_group(
    group: ServerGroup,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    copied_server_id: Option<String>,
    colors: CardColors,
) -> impl IntoElement {
    let servers_owned = group.servers.clone();
    let server_count = servers_owned.len();
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);
    div()
        .flex()
        .flex_col()
        .gap_3()
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
                        .child(div().w_1().h_5().bg(colors.primary).rounded_sm())
                        .child(render_icon(group.icon_path, colors.muted_foreground.into()))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .child(group.name.clone()),
                        ),
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors.header_bg)
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(format!(
                            "{} {}",
                            server_count,
                            i18n::t(&lang, "server_list.group_servers")
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_4()
                .children(servers_owned.into_iter().map(move |server| {
                    let state = dialog_state.clone();
                    let sess = session_state.clone();
                    let copied = copied_server_id.as_deref() == Some(server.id.as_str());
                    render_server_card(server, state, sess, view_state.clone(), copied, colors)
                })),
        )
}

/// 渲染服务器卡片
fn render_server_card(
    server: Server,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    copy_succeeded: bool,
    colors: CardColors,
) -> impl IntoElement {
    let server_id = server.id.clone();
    let server_id_for_edit = server_id.clone();
    let server_id_for_delete = server_id.clone();
    let server_id_for_connect = server_id.clone();
    let server_label_for_connect = server.name.clone();
    let dialog_for_edit = dialog_state.clone();
    let dialog_for_delete = dialog_state;
    let session_for_connect = session_state.clone();
    let session_for_button = session_state;
    let server_id_for_button = server_id.clone();
    let server_label_for_button = server.name.clone();
    let endpoint = format!("{}@{}:{}", server.account, server.host, server.port);
    let endpoint_for_copy = endpoint.clone();
    let server_id_for_copy = server_id.clone();
    let view_state_for_copy = view_state;
    let has_description = !server.description.trim().is_empty();
    div()
        .id(SharedString::from(format!("card-{}", server_id)))
        .w(px(280.))
        .min_h(px(190.))
        .bg(colors.bg)
        .rounded_xl()
        .border_1()
        .border_color(colors.border)
        .p_5()
        .cursor_pointer()
        .hover(move |s| {
            s.border_color(colors.primary.opacity(0.65))
                .bg(colors.header_bg)
                .shadow_md()
        })
        // 双击连接服务器
        .on_double_click(move |_, _, cx| {
            session_for_connect.update(cx, |state, cx| {
                state.add_tab(
                    server_id_for_connect.clone(),
                    server_label_for_connect.clone(),
                );
                // 确保 Monitor 详情弹窗状态已创建
                state.ensure_monitor_detail_dialog(cx);
            });
        })
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div().flex().items_center().justify_between().child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w_11()
                            .h_11()
                            .rounded_lg()
                            .bg(colors.primary.opacity(0.12))
                            .border_1()
                            .border_color(colors.primary.opacity(0.2))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(render_distribution_icon(&server, 22., colors.primary)),
                    )
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(server.name.clone()),
                    ),
            ),
        )
        .child({
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(has_description.then(|| {
                    div()
                        .h(px(38.))
                        .text_sm()
                        .text_color(colors.muted_foreground)
                        .overflow_hidden()
                        .child(server.description.clone())
                }))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .bg(colors.header_bg)
                        .border_1()
                        .border_color(colors.border)
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_family("monospace")
                                .text_color(colors.foreground)
                                .child(endpoint.clone()),
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
                                .bg(if copy_succeeded {
                                    colors.success.opacity(0.14)
                                } else {
                                    transparent_black()
                                })
                                .hover(move |s| s.bg(colors.secondary_hover))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    copy_endpoint_with_feedback(
                                        endpoint_for_copy.clone(),
                                        server_id_for_copy.clone(),
                                        view_state_for_copy.clone(),
                                        cx,
                                    );
                                })
                                .child(render_icon(
                                    if copy_succeeded {
                                        icons::CHECK
                                    } else {
                                        icons::COPY
                                    },
                                    if copy_succeeded {
                                        colors.success
                                    } else {
                                        colors.muted_foreground
                                    },
                                )),
                        ),
                )
        })
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(server.last_connected.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "card-connect-{}",
                                    server_id_for_button
                                )))
                                .w_8()
                                .h_8()
                                .rounded_md()
                                .bg(colors.primary)
                                .cursor_pointer()
                                .hover(move |s| s.bg(colors.primary.opacity(0.86)))
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    session_for_button.update(cx, |state, cx| {
                                        state.add_tab(
                                            server_id_for_button.clone(),
                                            server_label_for_button.clone(),
                                        );
                                        state.ensure_monitor_detail_dialog(cx);
                                    });
                                })
                                .child(render_icon(icons::PLAY, rgb(0xffffff).into())),
                        )
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
                                        error!("Failed to delete server: {}", e);
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
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    copied_server_id: Option<String>,
    colors: CardColors,
) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let servers_owned = group.servers.clone();
    let server_count = servers_owned.len();
    div()
        .bg(colors.bg)
        .rounded_xl()
        .border_1()
        .border_color(colors.border)
        .overflow_hidden()
        .child(
            div()
                .px_5()
                .py_4()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().w_1().h_5().bg(colors.primary).rounded_sm())
                        .child(render_icon(group.icon_path, colors.muted_foreground.into()))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .child(group.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(format!(
                            "{} {}",
                            server_count,
                            i18n::t(&lang, "server_list.group_servers")
                        )),
                ),
        )
        .child(
            div()
                .px_5()
                .py_3()
                .bg(colors.header_bg)
                .border_t_1()
                .border_b_1()
                .border_color(colors.border)
                .flex()
                .child(
                    div()
                        .w(px(220.))
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.server")),
                )
                .child(
                    div()
                        .w(px(260.))
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.host")),
                )
                .child(div().flex_1().min_w(px(0.)))
                .child(
                    div()
                        .w(px(170.))
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(i18n::t(&lang, "server_list.header.last_connected")),
                )
                .child(
                    div()
                        .w(px(112.))
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
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
                    let sess = session_state.clone();
                    let copied = copied_server_id.as_deref() == Some(server.id.as_str());
                    render_server_row(server, state, sess, view_state.clone(), copied, colors)
                })),
        )
}

/// 渲染服务器行
fn render_server_row(
    server: Server,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
    view_state: Entity<ViewModeState>,
    copy_succeeded: bool,
    colors: CardColors,
) -> impl IntoElement {
    let server_id = server.id.clone();
    let server_id_for_edit = server_id.clone();
    let server_id_for_delete = server_id.clone();
    let server_id_for_connect = server_id.clone();
    let server_label_for_connect = server.name.clone();
    let dialog_for_edit = dialog_state.clone();
    let dialog_for_delete = dialog_state;
    let session_for_connect = session_state.clone();
    let session_for_button = session_state;
    let server_id_for_button = server_id.clone();
    let server_label_for_button = server.name.clone();
    let endpoint = format!("{}@{}:{}", server.account, server.host, server.port);
    let endpoint_for_copy = endpoint.clone();
    let server_id_for_copy = server_id.clone();
    let view_state_for_copy = view_state;
    let has_description = !server.description.trim().is_empty();
    div()
        .id(SharedString::from(format!("row-{}", server_id)))
        .px_5()
        .py_3()
        .border_b_1()
        .border_color(colors.border.opacity(0.65))
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(move |s| s.bg(colors.header_bg))
        // 双击连接服务器
        .on_double_click(move |_, _, cx| {
            session_for_connect.update(cx, |state, cx| {
                state.add_tab(
                    server_id_for_connect.clone(),
                    server_label_for_connect.clone(),
                );
                // 确保 Monitor 详情弹窗状态已创建
                state.ensure_monitor_detail_dialog(cx);
            });
        })
        .child(
            div()
                .w(px(220.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .w_8()
                        .h_8()
                        .rounded_md()
                        .bg(colors.primary.opacity(0.12))
                        .border_1()
                        .border_color(colors.primary.opacity(0.2))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_distribution_icon(&server, 20., colors.primary)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .overflow_hidden()
                                .child(server.name.clone()),
                        )
                        .children(has_description.then(|| {
                            div()
                                .overflow_hidden()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(server.description.clone())
                        })),
                ),
        )
        .child({
            div()
                .w(px(260.))
                .min_w(px(0.))
                .overflow_hidden()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(colors.foreground)
                        .child(endpoint.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "copy-host-{}",
                            server_id_for_copy
                        )))
                        .cursor_pointer()
                        .p_1()
                        .rounded_sm()
                        .flex_shrink_0()
                        .bg(if copy_succeeded {
                            colors.success.opacity(0.14)
                        } else {
                            transparent_black()
                        })
                        .hover(move |s| s.bg(colors.secondary_hover))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            copy_endpoint_with_feedback(
                                endpoint_for_copy.clone(),
                                server_id_for_copy.clone(),
                                view_state_for_copy.clone(),
                                cx,
                            );
                        })
                        .child(render_icon(
                            if copy_succeeded {
                                icons::CHECK
                            } else {
                                icons::COPY
                            },
                            if copy_succeeded {
                                colors.success
                            } else {
                                colors.muted_foreground
                            },
                        )),
                )
        })
        .child(div().flex_1().min_w(px(0.)))
        .child(
            div()
                .w(px(170.))
                .flex_shrink_0()
                .overflow_hidden()
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(server.last_connected.clone()),
        )
        .child(
            div()
                .w(px(112.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-connect-{}",
                            server_id_for_button
                        )))
                        .w_8()
                        .h_8()
                        .rounded_md()
                        .bg(colors.primary.opacity(0.12))
                        .cursor_pointer()
                        .hover(move |s| s.bg(colors.primary.opacity(0.2)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            session_for_button.update(cx, |state, cx| {
                                state.add_tab(
                                    server_id_for_button.clone(),
                                    server_label_for_button.clone(),
                                );
                                state.ensure_monitor_detail_dialog(cx);
                            });
                        })
                        .child(render_icon(icons::PLAY, colors.primary.into())),
                )
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
                                error!("Failed to delete server: {}", e);
                            }
                            dialog_for_delete.update(cx, |s, _| {
                                s.needs_refresh = true;
                            });
                        })
                        .child(render_icon(icons::TRASH, colors.destructive.into())),
                ),
        )
}

fn copy_endpoint_with_feedback(
    endpoint: String,
    server_id: String,
    view_state: Entity<ViewModeState>,
    cx: &mut App,
) {
    cx.write_to_clipboard(ClipboardItem::new_string(endpoint));

    let token = view_state.read(cx).copy_feedback_token.wrapping_add(1);
    view_state.update(cx, |state, cx| {
        state.copied_server_id = Some(server_id.clone());
        state.copy_feedback_token = token;
        cx.notify();
    });

    cx.spawn(async move |async_cx| {
        async_cx
            .background_executor()
            .timer(std::time::Duration::from_millis(1200))
            .await;

        let _ = async_cx.update(|cx| {
            view_state.update(cx, |state, cx| {
                if state.copy_feedback_token == token
                    && state.copied_server_id.as_deref() == Some(server_id.as_str())
                {
                    state.copied_server_id = None;
                    cx.notify();
                }
            });
        });
    })
    .detach();
}

/// 优先使用连接后采集到的 OS；未采集时从名称和描述中做保守识别。
fn distribution_icon(server: &Server) -> Option<&'static str> {
    detect_distribution_icon(server.os_name.as_deref(), &server.name, &server.description)
}

fn render_distribution_icon(server: &Server, size: f32, fallback_color: Hsla) -> AnyElement {
    match distribution_icon(server) {
        Some(path) => render_colored_icon(path, size).into_any_element(),
        None => render_icon(icons::SERVER, fallback_color).into_any_element(),
    }
}

fn render_search_empty(query: &str, cx: &App) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .child(
            div()
                .w_16()
                .h_16()
                .rounded_2xl()
                .bg(cx.theme().secondary)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(
                    icons::SEARCH,
                    cx.theme().muted_foreground.into(),
                )),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(i18n::t(&lang, "server_list.no_results_title")),
        )
        .child(
            div()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(cx.theme().popover)
                .border_1()
                .border_color(cx.theme().border)
                .font_family("monospace")
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(query.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(i18n::t(&lang, "server_list.no_results_description")),
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
        .gap_5()
        .child(
            div()
                .w_20()
                .h_20()
                .rounded_2xl()
                .bg(cx.theme().popover)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w_12()
                        .h_12()
                        .rounded_xl()
                        .bg(cx.theme().primary.opacity(0.12))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::SERVER, cx.theme().primary.into())),
                ),
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
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
                .rounded_xl()
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

#[cfg(test)]
mod tests {
    use super::{distribution_icon, filter_server_groups};
    use crate::constants::icons;
    use crate::models::{Server, ServerGroup};

    fn server(name: &str, host: &str, account: &str, port: u16) -> Server {
        Server {
            id: name.to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port,
            description: format!("{} node", name),
            account: account.to_string(),
            os_name: None,
            last_connected: "Never".to_string(),
        }
    }

    fn groups() -> Vec<ServerGroup> {
        vec![
            ServerGroup {
                name: "Production".to_string(),
                icon_path: icons::SERVER,
                servers: vec![server("API", "10.0.0.10", "root", 22)],
            },
            ServerGroup {
                name: "Staging".to_string(),
                icon_path: icons::SERVER,
                servers: vec![server("Worker", "staging.internal", "deploy", 2202)],
            },
        ]
    }

    #[test]
    fn empty_query_keeps_all_servers() {
        let filtered = filter_server_groups(&groups(), "  ");
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered.iter().map(|g| g.servers.len()).sum::<usize>(), 2);
    }

    #[test]
    fn search_matches_server_fields_case_insensitively() {
        let filtered = filter_server_groups(&groups(), "STAGING.INTERNAL");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].servers[0].name, "Worker");

        let filtered = filter_server_groups(&groups(), "2202");
        assert_eq!(filtered[0].servers[0].account, "deploy");
    }

    #[test]
    fn matching_group_name_keeps_the_whole_group() {
        let filtered = filter_server_groups(&groups(), "production");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Production");
        assert_eq!(filtered[0].servers.len(), 1);
    }

    #[test]
    fn distribution_prefers_collected_os_name() {
        let mut host = server("Production", "10.0.0.10", "root", 22);
        host.os_name = Some("Ubuntu 24.04.1 LTS".to_string());
        assert_eq!(distribution_icon(&host), Some(icons::DISTRO_UBUNTU));
    }

    #[test]
    fn distribution_falls_back_to_profile_text() {
        let host = server("Debian backup", "10.0.0.11", "root", 22);
        assert_eq!(distribution_icon(&host), Some(icons::DISTRO_DEBIAN));

        let unknown = server("Database", "10.0.0.12", "root", 22);
        assert_eq!(distribution_icon(&unknown), None);
    }
}
