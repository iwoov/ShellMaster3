// Session 右侧边栏：快捷命令 / 传输 / AI Agent 三个面板，各带 36px 标题条

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use std::sync::Arc;
use tracing::debug;

use crate::components::common::button::{icon_button_sized, text_tooltip};
use crate::components::common::format::{format_bytes, format_speed};
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{
    meter_bar, panel_header, panel_state, panel_title, StateIcon, LIST_ROW_HEIGHT, PANEL_PADDING,
};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::{TransferItem, TransferStatus};
use crate::models::{SnippetCommand, SnippetsConfig};
use crate::state::{SessionState, SessionTab, SidebarPanel};
use crate::theme::Palette;

/// 渲染会话右侧边栏。`width` 是侧栏宽度（含左边线）
#[allow(clippy::too_many_arguments)]
pub fn render_session_sidebar(
    tab: &SessionTab,
    active_panel: SidebarPanel,
    width: Pixels,
    session_state: Entity<SessionState>,
    lang: &Language,
    fit: &TextFitter,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    // 内容可用宽度：去掉左边线
    let width = (width - px(1.)).max(px(0.));

    let content = match active_panel {
        SidebarPanel::Snippets => {
            render_snippets_panel(&session_state, width, lang, fit, &p, cx).into_any_element()
        }
        SidebarPanel::Transfer => {
            render_transfer_panel(tab, &session_state, width, lang, fit, &p).into_any_element()
        }
        SidebarPanel::AiChat => {
            super::agent_panel::render_agent_panel(tab, width, session_state.clone(), lang, window, cx).into_any_element()
        }
        SidebarPanel::PortForward => {
            render_port_forward_panel(tab, &session_state, lang, &p, window, cx).into_any_element()
        }
    };

    div()
        .size_full()
        .bg(p.sidebar)
        .flex()
        .flex_col()
        .child(content)
}

/// 面板标题条：标题 + 弱色的数量
fn sidebar_header(title: &'static str, count: Option<usize>, p: &Palette) -> Div {
    panel_header(p)
        .bg(p.sidebar)
        .pl(PANEL_PADDING)
        .pr(px(8.))
        .gap(px(6.))
        .child(panel_title(title, p))
        .children(count.map(|count| {
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(count.to_string())
        }))
        .child(div().flex_1())
}

// ==================== 传输面板 ====================

/// 传输行里用于联动悬停样式的分组名
const TRANSFER_ROW_GROUP: &str = "transfer-row";
/// 传输方向图标块的边长
const TRANSFER_TILE: f32 = 28.;

/// 渲染传输管理面板（当前标签的上传 / 下载任务）
fn render_transfer_panel(
    tab: &SessionTab,
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let transfers = &tab.active_transfers;
    let header = sidebar_header(
        i18n::t(lang, "transfer.title"),
        (!transfers.is_empty()).then_some(transfers.len()),
        p,
    );

    let body = if transfers.is_empty() {
        panel_state(
            "transfer-empty",
            StateIcon::Icon(icons::TRANSFER),
            p.text_faint,
            i18n::t(lang, "transfer.empty"),
            None,
            p,
        )
        .into_any_element()
    } else {
        div()
            .id("transfer-list-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .child(div().flex().flex_col().children(
                transfers.iter().map(|transfer| {
                    render_transfer_row(transfer, session_state, width, lang, fit, p)
                }),
            ))
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(body)
}

/// 传输状态对应的文字
fn transfer_status_label(transfer: &TransferItem, lang: &Language) -> &'static str {
    let key = match transfer.status {
        TransferStatus::Pending => "transfer.status.pending",
        // 按方向显示：上传不会显示成「下载中」
        TransferStatus::Uploading | TransferStatus::Downloading if transfer.is_upload => {
            "transfer.status.uploading"
        }
        TransferStatus::Uploading | TransferStatus::Downloading => "transfer.status.downloading",
        TransferStatus::Paused => "transfer.status.paused",
        TransferStatus::Completed => "transfer.status.completed",
        TransferStatus::Failed => "transfer.status.failed",
        TransferStatus::Cancelled => "transfer.status.cancelled",
    };
    i18n::t(lang, key)
}

/// 进度（0–1）：已完成的任务一律算满，0 字节文件也是
fn transfer_fraction(transfer: &TransferItem) -> f32 {
    if transfer.status == TransferStatus::Completed {
        1.
    } else {
        (transfer.progress.percentage() / 100.) as f32
    }
}

/// 传输行的第二行文字：进度、速度或状态说明
fn transfer_detail(transfer: &TransferItem, lang: &Language) -> String {
    let progress = &transfer.progress;
    let sizes = format!(
        "{} / {}",
        format_bytes(progress.bytes_transferred),
        format_bytes(progress.total_bytes)
    );
    match transfer.status {
        TransferStatus::Uploading | TransferStatus::Downloading
            if progress.speed_bytes_per_sec > 0 =>
        {
            format!(
                "{sizes} · {}",
                format_speed(progress.speed_bytes_per_sec as f64)
            )
        }
        TransferStatus::Uploading | TransferStatus::Downloading => sizes,
        TransferStatus::Completed => format!(
            "{} · {}",
            transfer_status_label(transfer, lang),
            format_bytes(progress.total_bytes)
        ),
        TransferStatus::Failed => match &transfer.error {
            Some(error) if !error.is_empty() => {
                format!("{}：{}", transfer_status_label(transfer, lang), error)
            }
            _ => transfer_status_label(transfer, lang).to_string(),
        },
        TransferStatus::Paused => format!("{} · {sizes}", transfer_status_label(transfer, lang)),
        TransferStatus::Pending | TransferStatus::Cancelled => {
            transfer_status_label(transfer, lang).to_string()
        }
    }
}

/// 一条传输：方向图标块、文件名、进度说明、百分比（悬停时换成暂停 / 继续、取消按钮）、进度条
fn render_transfer_row(
    transfer: &TransferItem,
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let tone = if transfer.is_upload {
        p.primary
    } else {
        p.success
    };
    let (bar_color, detail_color) = match transfer.status {
        TransferStatus::Completed => (p.success, p.text_faint),
        TransferStatus::Failed => (p.danger, p.danger),
        TransferStatus::Paused | TransferStatus::Cancelled | TransferStatus::Pending => {
            (p.text_faint, p.text_faint)
        }
        _ => (p.primary, p.text_faint),
    };
    let is_active = transfer.status.is_active();
    let is_paused = transfer.status == TransferStatus::Paused;
    let percent = format!("{:.0}%", transfer_fraction(transfer) * 100.);

    // 文字可用宽度：去掉左右留白、图标块、间距和右侧百分比 / 按钮
    let text_width = (width - PANEL_PADDING * 2. - px(TRANSFER_TILE + 10. + 8. + 52.)).max(px(40.));
    let name = transfer.file_name();
    let detail = transfer_detail(transfer, lang);

    let actions = is_active.then(|| {
        let pause_state = session_state.clone();
        let cancel_state = session_state.clone();
        let pause_id = transfer.id.clone();
        let cancel_id = transfer.id.clone();
        div()
            .absolute()
            .top_0()
            .right_0()
            .flex()
            .gap(px(2.))
            .invisible()
            .group_hover(TRANSFER_ROW_GROUP, |s| s.visible())
            .child(
                icon_button_sized(
                    SharedString::from(format!("transfer-pause-{}", transfer.id)),
                    if is_paused { icons::PLAY } else { icons::PAUSE },
                    px(24.),
                    12.,
                    i18n::t(
                        lang,
                        if is_paused {
                            "transfer.resume"
                        } else {
                            "transfer.pause"
                        },
                    ),
                    p,
                )
                .on_click(move |_, _, cx| {
                    pause_state.update(cx, |state, cx| {
                        if is_paused {
                            state.resume_transfer(&pause_id, cx);
                        } else {
                            state.pause_transfer(&pause_id, cx);
                        }
                    });
                }),
            )
            .child(
                icon_button_sized(
                    SharedString::from(format!("transfer-cancel-{}", transfer.id)),
                    icons::X,
                    px(24.),
                    12.,
                    i18n::t(lang, "transfer.cancel"),
                    p,
                )
                .on_click(move |_, _, cx| {
                    cancel_state.update(cx, |state, cx| {
                        state.cancel_transfer(&cancel_id, cx);
                    });
                }),
            )
    });

    div()
        .id(SharedString::from(format!("transfer-{}", transfer.id)))
        .group(TRANSFER_ROW_GROUP)
        .px(PANEL_PADDING)
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .border_b_1()
        .border_color(p.line)
        .hover(|s| s.bg(p.hover))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(TRANSFER_TILE))
                        .rounded(px(7.))
                        .bg(tone.opacity(0.14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon_sized(
                            if transfer.is_upload {
                                icons::UPLOAD
                            } else {
                                icons::DOWNLOAD
                            },
                            14.,
                            tone,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            fit.label_with_tooltip(
                                SharedString::from(format!("transfer-name-{}", transfer.id)),
                                &name,
                                text_width,
                                13.,
                                FontWeight::MEDIUM,
                            )
                            .text_color(p.text),
                        )
                        .child(
                            fit.label_with_tooltip(
                                SharedString::from(format!("transfer-detail-{}", transfer.id)),
                                &detail,
                                text_width,
                                11.,
                                FontWeight::NORMAL,
                            )
                            .text_color(detail_color),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .w(px(52.))
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .when(is_active, |text| {
                                    text.group_hover(TRANSFER_ROW_GROUP, |s| s.invisible())
                                })
                                .child(percent),
                        )
                        .children(actions),
                ),
        )
        .child(meter_bar(transfer_fraction(transfer), bar_color, 3., p))
}

// ==================== 快捷命令面板 ====================

/// 每一级缩进的宽度
const SNIPPET_INDENT: f32 = 14.;
/// 箭头所在方框、图标的尺寸和它们之间的间距
const SNIPPET_CHEVRON: f32 = 12.;
const SNIPPET_ICON: f32 = 16.;
const SNIPPET_GAP: f32 = 6.;
/// 列表与行的左右留白
const SNIPPET_LIST_PADDING: f32 = 6.;
const SNIPPET_ROW_PADDING: f32 = 6.;
/// 行内用于联动悬停样式的分组名
const SNIPPET_ROW_GROUP: &str = "snippet-row";

/// 快捷命令树展开后的一行
enum SnippetRow {
    Group {
        id: String,
        name: String,
        expanded: bool,
        /// 组内（含子组）的命令数
        count: usize,
        has_children: bool,
        depth: usize,
    },
    Command {
        command: SnippetCommand,
        depth: usize,
    },
}

/// 一个组（含所有子组）里的命令总数
fn count_commands(config: &SnippetsConfig, group_id: &str) -> usize {
    config.get_commands_in_group(Some(group_id)).len()
        + config
            .get_child_groups(Some(group_id))
            .into_iter()
            .map(|child| count_commands(config, &child.id))
            .sum::<usize>()
}

/// 把快捷命令树按显示顺序展开成行：每层先列组，再列命令；只展开已展开的组
fn flatten_snippets(
    config: &SnippetsConfig,
    expanded: &std::collections::HashSet<String>,
    parent: Option<&str>,
    depth: usize,
    rows: &mut Vec<SnippetRow>,
) {
    for group in config.get_child_groups(parent) {
        let is_expanded = expanded.contains(&group.id);
        let has_children = !config.get_child_groups(Some(&group.id)).is_empty()
            || !config.get_commands_in_group(Some(&group.id)).is_empty();
        rows.push(SnippetRow::Group {
            id: group.id.clone(),
            name: group.name.clone(),
            expanded: is_expanded,
            count: count_commands(config, &group.id),
            has_children,
            depth,
        });
        if is_expanded {
            flatten_snippets(config, expanded, Some(&group.id), depth + 1, rows);
        }
    }
    for command in config.get_commands_in_group(parent) {
        rows.push(SnippetRow::Command {
            command: command.clone(),
            depth,
        });
    }
}

/// 某一层的行左侧缩进（不含行内留白）
fn snippet_indent(depth: usize) -> f32 {
    SNIPPET_INDENT * depth as f32
}

/// 渲染快捷命令面板
fn render_snippets_panel(
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let state = session_state.read(cx);
    let config = state.snippets_config.clone().unwrap_or_default();
    let expanded = state.snippets_expanded.clone();

    let mut rows = Vec::new();
    flatten_snippets(&config, &expanded, None, 0, &mut rows);

    let header = sidebar_header(
        i18n::t(lang, "session.rail.snippets"),
        (!config.commands.is_empty()).then_some(config.commands.len()),
        p,
    );

    let body = if rows.is_empty() {
        panel_state(
            "snippets-empty",
            StateIcon::Icon(icons::CODE),
            p.text_faint,
            i18n::t(lang, "snippets.empty.title"),
            Some(i18n::t(lang, "snippets.empty.session_hint").into()),
            p,
        )
        .into_any_element()
    } else {
        // 当前终端的 PTY：右键菜单「在终端执行」用
        let pty_channel: Option<Arc<crate::ssh::session::TerminalChannel>> = state
            .active_tab()
            .and_then(|tab| {
                tab.active_terminal_id
                    .as_ref()
                    .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
            })
            .and_then(|inst| inst.pty_channel.clone());

        div()
            .id("snippets-tree-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .p(px(SNIPPET_LIST_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows.into_iter().map(|row| {
                        match row {
                            SnippetRow::Group {
                                id,
                                name,
                                expanded,
                                count,
                                has_children,
                                depth,
                            } => render_group_row(
                                &id,
                                &name,
                                expanded,
                                count,
                                has_children,
                                depth,
                                session_state,
                                width,
                                fit,
                                p,
                            )
                            .into_any_element(),
                            SnippetRow::Command { command, depth } => render_command_row(
                                command,
                                depth,
                                session_state,
                                pty_channel.clone(),
                                width,
                                lang,
                                fit,
                                p,
                                cx,
                            )
                            .into_any_element(),
                        }
                    })),
            )
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(body)
}

/// 名称可用宽度：侧栏宽度减去列表和行的留白、缩进、箭头、图标和右侧数量
fn snippet_name_width(width: Pixels, depth: usize, trailing: Pixels) -> Pixels {
    let used = SNIPPET_LIST_PADDING * 2.
        + SNIPPET_ROW_PADDING * 2.
        + snippet_indent(depth)
        + SNIPPET_CHEVRON
        + SNIPPET_GAP
        + SNIPPET_ICON
        + SNIPPET_GAP;
    (width - px(used) - trailing).max(px(24.))
}

/// 组的一行：箭头、文件夹图标、名称和命令数，点击展开 / 收起
#[allow(clippy::too_many_arguments)]
fn render_group_row(
    id: &str,
    name: &str,
    expanded: bool,
    count: usize,
    has_children: bool,
    depth: usize,
    session_state: &Entity<SessionState>,
    width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let session_state = session_state.clone();
    let group_id = id.to_string();
    let count_text = count.to_string();
    let count_width = fit.width(&count_text, 11., FontWeight::NORMAL) + px(SNIPPET_GAP);
    let (hover_bg, hover_text) = (p.hover, p.text);

    div()
        .id(SharedString::from(format!("snippet-group-{id}")))
        .group(SNIPPET_ROW_GROUP)
        .flex_none()
        .h(LIST_ROW_HEIGHT)
        .pl(px(SNIPPET_ROW_PADDING + snippet_indent(depth)))
        .pr(px(SNIPPET_ROW_PADDING))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(SNIPPET_GAP))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.toggle_snippets_group(&group_id);
                cx.notify();
            });
        })
        .child(
            icon_sized(
                if expanded {
                    icons::CHEVRON_DOWN
                } else {
                    icons::CHEVRON_RIGHT
                },
                SNIPPET_CHEVRON,
                if has_children {
                    p.text_faint
                } else {
                    p.text_faint.opacity(0.4)
                },
            )
            .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text)),
        )
        .child(icon_sized(
            if expanded {
                icons::FOLDER_OPEN
            } else {
                icons::FOLDER
            },
            SNIPPET_ICON,
            p.text_muted,
        ))
        .child(
            fit.label_with_tooltip(
                SharedString::from(format!("snippet-group-name-{id}")),
                name,
                snippet_name_width(width, depth, count_width),
                13.,
                FontWeight::MEDIUM,
            )
            .flex_none()
            .text_color(p.text),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(p.text_faint)
                .child(count_text),
        )
}

/// 命令的一行：图标与同级组的文件夹图标对齐，点击复制命令（图标短暂变成对勾），右键可执行或放进命令框
#[allow(clippy::too_many_arguments)]
fn render_command_row(
    command: SnippetCommand,
    depth: usize,
    session_state: &Entity<SessionState>,
    pty_channel: Option<Arc<crate::ssh::session::TerminalChannel>>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let copy_key = format!("snippet-{}", command.id);
    let copied = session_state.read(cx).is_copied(&copy_key);
    let (hover_bg, hover_text) = (p.hover, p.text);

    let command_text = command.command.clone();
    let command_for_execute = command.command.clone();
    let command_for_edit = command.command.clone();
    let execute_label = i18n::t(lang, "snippets.context_menu.execute");
    let edit_label = i18n::t(lang, "snippets.context_menu.edit_in_box");
    let session_for_copy = session_state.clone();
    let session_for_menu = session_state.clone();

    div()
        .id(SharedString::from(format!("cmd-{}", command.id)))
        .group(SNIPPET_ROW_GROUP)
        .flex_none()
        .h(LIST_ROW_HEIGHT)
        // 命令没有箭头：留出箭头的位置，图标与同级组的文件夹图标对齐
        .pl(px(SNIPPET_ROW_PADDING
            + snippet_indent(depth)
            + SNIPPET_CHEVRON
            + SNIPPET_GAP))
        .pr(px(SNIPPET_ROW_PADDING))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(SNIPPET_GAP))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            SessionState::copy_with_feedback(
                &session_for_copy,
                copy_key.clone(),
                command_text.clone(),
                cx,
            );
        })
        // 悬停显示完整命令
        .tooltip(text_tooltip(command.command.clone()))
        .context_menu(move |menu, _window, _cx| {
            let command_for_execute = command_for_execute.clone();
            let command_for_edit = command_for_edit.clone();
            let pty_for_menu = pty_channel.clone();
            let session_for_menu = session_for_menu.clone();
            menu.item(snippet_menu_item(icons::PLAY, execute_label).on_click(
                move |_, _window, _cx| {
                    if let Some(channel) = &pty_for_menu {
                        let channel = Arc::clone(channel);
                        debug!("[ContextMenu] Executing command: {}", command_for_execute);
                        let mut bytes = command_for_execute.clone().into_bytes();
                        bytes.push(0x0d); // CR
                        if let Err(e) = channel.queue_write(bytes) {
                            tracing::error!("[ContextMenu] Failed to send command: {:?}", e);
                        }
                    }
                },
            ))
            .item(
                snippet_menu_item(icons::EDIT, edit_label).on_click(move |_, window, cx| {
                    let command = command_for_edit.clone();
                    debug!("[ContextMenu] Edit in command box: {}", command);
                    session_for_menu.update(cx, |state, cx| {
                        state.set_command_input_text(command, window, cx);
                    });
                }),
            )
        })
        .child(
            div()
                .flex_none()
                .size(px(SNIPPET_ICON))
                .flex()
                .items_center()
                .justify_center()
                .child(if copied {
                    icon_sized(icons::CHECK, 14., p.success)
                } else {
                    icon_sized(icons::CODE, 14., p.text_faint)
                        .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text))
                }),
        )
        .child(
            fit.label(
                &command.name,
                snippet_name_width(width, depth, px(0.)),
                13.,
                FontWeight::NORMAL,
            )
            .flex_none()
            .text_color(p.text_muted)
            .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text)),
        )
}

/// 快捷命令右键菜单的一项：图标 + 文字
fn snippet_menu_item(icon: &'static str, label: &'static str) -> PopupMenuItem {
    PopupMenuItem::element(move |_window, cx| {
        let p = crate::theme::palette(cx);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon_sized(icon, 14., p.text_muted))
            .child(div().text_size(px(13.)).text_color(p.text).child(label))
    })
}

/// 渲染 SSH 端口转发（-L / -R / -D）侧边栏面板
fn render_port_forward_panel(
    tab: &SessionTab,
    session_state: &Entity<SessionState>,
    lang: &Language,
    p: &Palette,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    use crate::components::common::button::{text_button, ButtonKind};
    use crate::components::common::input::{segmented, text_field};
    use crate::components::common::panel::status_dot;
    use crate::ssh::forwarding::ForwardingKind;

    session_state.update(cx, |state, cx| {
        state.ensure_port_forward_inputs(window, cx);
    });

    let state_read = session_state.read(cx);
    let kind = state_read.forward_kind;
    let bind_host_input = state_read.forward_bind_host_input.clone();
    let bind_port_input = state_read.forward_bind_port_input.clone();
    let target_host_input = state_read.forward_target_host_input.clone();
    let target_port_input = state_read.forward_target_port_input.clone();
    let error_msg = state_read.forward_error.clone();
    let active_count = tab.port_forwards.len();

    let header = sidebar_header(
        i18n::t(lang, "port_forward.title"),
        (active_count > 0).then_some(active_count),
        p,
    );

    let session_for_kind = session_state.clone();
    let kind_picker = segmented(
        "port-forward-kind",
        vec![
            (
                ForwardingKind::Local,
                i18n::t(lang, "port_forward.kind.local").into(),
            ),
            (
                ForwardingKind::Remote,
                i18n::t(lang, "port_forward.kind.remote").into(),
            ),
            (
                ForwardingKind::Dynamic,
                i18n::t(lang, "port_forward.kind.dynamic").into(),
            ),
        ],
        &kind,
        true,
        move |next_kind, _, cx| {
            session_for_kind.update(cx, |s, cx| {
                s.forward_kind = next_kind;
                s.forward_error = None;
                cx.notify();
            });
        },
        p,
    );

    let field_box = |label: &'static str,
                     input: Option<Entity<gpui_kit::component::input::InputState>>,
                     p: &Palette| {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.text_muted)
                    .child(label),
            )
            .children(input.as_ref().map(text_field))
    };

    let session_for_start = session_state.clone();
    let tab_id_for_start = tab.id.clone();
    let form = div()
        .p(PANEL_PADDING)
        .border_b_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(kind_picker)
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(field_box(
                    i18n::t(lang, "port_forward.bind_host"),
                    bind_host_input,
                    p,
                ))
                .child(field_box(
                    i18n::t(lang, "port_forward.bind_port"),
                    bind_port_input,
                    p,
                )),
        )
        .when(kind != ForwardingKind::Dynamic, |this| {
            this.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(field_box(
                        i18n::t(lang, "port_forward.target_host"),
                        target_host_input,
                        p,
                    ))
                    .child(field_box(
                        i18n::t(lang, "port_forward.target_port"),
                        target_port_input,
                        p,
                    )),
            )
        })
        .children(error_msg.map(|err| {
            div()
                .text_size(px(11.5))
                .text_color(p.danger)
                .child(err)
        }))
        .child(
            text_button(
                "port-forward-start-btn",
                ButtonKind::Primary,
                i18n::t(lang, "port_forward.start"),
                p,
            )
            .w_full()
            .justify_center()
            .on_click(move |_, _, cx| {
                session_for_start.update(cx, |s, cx| {
                    s.start_port_forward_from_form(&tab_id_for_start, cx);
                });
            }),
        );

    let list_body: AnyElement = if tab.port_forwards.is_empty() {
        panel_state(
            "port-forward-empty",
            StateIcon::Icon(icons::LINK),
            p.text_faint,
            i18n::t(lang, "port_forward.empty"),
            Some(i18n::t(lang, "port_forward.empty_hint").into()),
            p,
        )
        .into_any_element()
    } else {
        let mut items = div()
            .id("port-forward-list")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .p(PANEL_PADDING)
            .flex()
            .flex_col()
            .gap(px(8.));

        for rule in &tab.port_forwards {
            let copy_key = format!("pf-{}", rule.id);
            let copied = session_state.read(cx).is_copied(&copy_key);
            let endpoint = rule.listen_endpoint();
            let session_for_copy = session_state.clone();
            let session_for_stop = session_state.clone();
            let tab_id_for_stop = tab.id.clone();
            let rule_id_for_stop = rule.id.clone();

            items = items.child(
                div()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(status_dot(p.success, 7.))
                    .child(
                        div()
                            .px(px(5.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .bg(p.muted)
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.primary)
                            .child(rule.kind.short_tag()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(12.))
                            .text_color(p.text)
                            .child(rule.display_route()),
                    )
                    .child(
                        icon_button_sized(
                            SharedString::from(format!("pf-copy-{}", rule.id)),
                            if copied { icons::CHECK } else { icons::COPY },
                            px(24.),
                            13.,
                            i18n::t(lang, "port_forward.copy_endpoint"),
                            p,
                        )
                        .on_click(move |_, _, cx| {
                            SessionState::copy_with_feedback(
                                &session_for_copy,
                                copy_key.clone(),
                                endpoint.clone(),
                                cx,
                            );
                        }),
                    )
                    .child(
                        icon_button_sized(
                            SharedString::from(format!("pf-stop-{}", rule.id)),
                            icons::X,
                            px(24.),
                            13.,
                            i18n::t(lang, "port_forward.stop"),
                            p,
                        )
                        .on_click(move |_, _, cx| {
                            session_for_stop.update(cx, |s, cx| {
                                s.stop_port_forward(&tab_id_for_stop, &rule_id_for_stop, cx);
                            });
                        }),
                    ),
            );
        }
        items.into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(form)
        .child(div().flex_1().min_h(px(0.)).child(list_body))
}
