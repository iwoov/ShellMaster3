// Session 右侧边栏组件 - 快捷命令树状显示

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, StyledExt};
use std::sync::Arc;
use tracing::debug;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::models::{SnippetCommand, SnippetGroup, SnippetsConfig};
use crate::state::{SessionState, SessionTab, SidebarPanel};

/// 渲染会话右侧边栏
pub fn render_session_sidebar(
    _tab: &SessionTab,
    active_panel: SidebarPanel,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);
    let foreground = cx.theme().foreground;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 根据不同面板显示不同内容
    let (title, content) = match active_panel {
        SidebarPanel::Snippets => (
            crate::i18n::t(&lang, "mini_sidebar.snippets"),
            render_snippets_tree(session_state.clone(), cx).into_any_element(),
        ),
        SidebarPanel::Transfer => (
            crate::i18n::t(&lang, "mini_sidebar.transfer"),
            render_placeholder_content("传输管理功能待完善", muted_foreground).into_any_element(),
        ),
    };

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        .child(
            // 标题栏
            div()
                .w_full()
                .h(px(44.))
                .flex()
                .items_center()
                .px_4()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(foreground)
                        .child(title),
                ),
        )
        .child(
            // 内容区域
            content,
        )
}

/// 渲染占位内容
fn render_placeholder_content(text: &'static str, color: Hsla) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .child(div().text_sm().text_color(color).child(text))
}

/// 渲染快捷命令树
fn render_snippets_tree(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    // 获取配置
    let state = session_state.read(cx);
    let config = state.snippets_config.clone().unwrap_or_default();
    let expanded = state.snippets_expanded.clone();

    // 获取根级组和命令
    let root_groups = config.get_child_groups(None);
    let root_commands = config.get_commands_in_group(None);

    let has_items = !root_groups.is_empty() || !root_commands.is_empty();

    div()
        .id("snippets-tree-scroll")
        .flex_1()
        .overflow_y_scroll()
        .p_2()
        .child(if has_items {
            render_tree_nodes(
                session_state.clone(),
                &config,
                &expanded,
                root_groups,
                root_commands,
                0,
                cx,
            )
            .into_any_element()
        } else {
            render_empty_tree(cx).into_any_element()
        })
}

/// 渲染空状态
fn render_empty_tree(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .pt_8()
        .child(render_icon(icons::CODE, muted.into()))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(crate::i18n::t(&lang, "snippets.empty.title")),
        )
}

/// 递归渲染树节点
fn render_tree_nodes(
    session_state: Entity<SessionState>,
    config: &SnippetsConfig,
    expanded: &std::collections::HashSet<String>,
    groups: Vec<&SnippetGroup>,
    commands: Vec<&SnippetCommand>,
    level: usize,
    cx: &App,
) -> impl IntoElement {
    let indent = px((level * 16) as f32);
    let config_clone = config.clone();

    div()
        .flex()
        .flex_col()
        // 渲染文件夹节点
        .children(groups.into_iter().map(|group| {
            let group_owned = group.clone();
            let is_expanded = expanded.contains(&group.id);
            let child_groups = config_clone.get_child_groups(Some(&group.id));
            let child_commands = config_clone.get_commands_in_group(Some(&group.id));
            let has_children = !child_groups.is_empty() || !child_commands.is_empty();

            render_folder_node(
                session_state.clone(),
                group_owned,
                is_expanded,
                has_children,
                child_groups
                    .into_iter()
                    .map(|g| g.clone())
                    .collect::<Vec<_>>(),
                child_commands
                    .into_iter()
                    .map(|c| c.clone())
                    .collect::<Vec<_>>(),
                level,
                expanded.clone(),
                config_clone.clone(),
                cx,
            )
        }))
        // 渲染命令节点
        .children(commands.into_iter().map({
            let session_state = session_state.clone();
            move |command| {
                let command_owned = command.clone();
                render_command_node(command_owned, session_state.clone(), cx)
            }
        }))
        .ml(indent)
}

/// 渲染文件夹节点
fn render_folder_node(
    session_state: Entity<SessionState>,
    group: SnippetGroup,
    is_expanded: bool,
    has_children: bool,
    child_groups: Vec<SnippetGroup>,
    child_commands: Vec<SnippetCommand>,
    level: usize,
    expanded: std::collections::HashSet<String>,
    config: SnippetsConfig,
    cx: &App,
) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover_bg = cx.theme().list_active;
    let primary = cx.theme().primary;

    let group_id = group.id.clone();
    let group_id_for_click = group_id.clone();
    let session_for_click = session_state.clone();

    div()
        .flex()
        .flex_col()
        .child(
            // 文件夹行
            div()
                .id(SharedString::from(format!("folder-{}", group_id)))
                .h(px(28.))
                .px_1()
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    session_for_click.update(cx, |state, _| {
                        state.toggle_snippets_group(&group_id_for_click);
                    });
                })
                // 展开/折叠箭头
                .child(
                    svg()
                        .path(if is_expanded {
                            icons::CHEVRON_DOWN
                        } else {
                            icons::CHEVRON_RIGHT
                        })
                        .size(px(12.))
                        .text_color(if has_children {
                            muted
                        } else {
                            muted.opacity(0.3)
                        }),
                )
                // 文件夹图标
                .child(
                    svg()
                        .path(if is_expanded {
                            icons::FOLDER_OPEN
                        } else {
                            icons::FOLDER
                        })
                        .size(px(14.))
                        .text_color(primary),
                )
                // 文件夹名称
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(foreground)
                        .overflow_hidden()
                        .child(group.name.clone()),
                ),
        )
        // 展开时显示子项
        .when(is_expanded && has_children, |this| {
            this.child(render_tree_nodes_owned(
                session_state.clone(),
                config,
                expanded,
                child_groups,
                child_commands,
                level + 1,
                cx,
            ))
        })
}

/// 渲染树节点（拥有所有权版本）
fn render_tree_nodes_owned(
    session_state: Entity<SessionState>,
    config: SnippetsConfig,
    expanded: std::collections::HashSet<String>,
    groups: Vec<SnippetGroup>,
    commands: Vec<SnippetCommand>,
    level: usize,
    cx: &App,
) -> impl IntoElement {
    let indent = px((level * 16) as f32);

    div()
        .flex()
        .flex_col()
        .ml(indent)
        // 渲染文件夹节点
        .children(groups.into_iter().map(|group| {
            let is_expanded = expanded.contains(&group.id);
            let child_groups: Vec<SnippetGroup> = config
                .get_child_groups(Some(&group.id))
                .into_iter()
                .map(|g| g.clone())
                .collect();
            let child_commands: Vec<SnippetCommand> = config
                .get_commands_in_group(Some(&group.id))
                .into_iter()
                .map(|c| c.clone())
                .collect();
            let has_children = !child_groups.is_empty() || !child_commands.is_empty();

            render_folder_node(
                session_state.clone(),
                group,
                is_expanded,
                has_children,
                child_groups,
                child_commands,
                level,
                expanded.clone(),
                config.clone(),
                cx,
            )
        }))
        // 渲染命令节点
        .children({
            let session_state = session_state.clone();
            commands
                .into_iter()
                .map(move |command| render_command_node(command, session_state.clone(), cx))
        })
}

/// 渲染命令节点
fn render_command_node(
    command: SnippetCommand,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover_bg = cx.theme().list_active;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    let command_id = command.id.clone();
    let command_text = command.command.clone();
    let command_text_for_tooltip = command.command.clone();
    let command_text_for_execute = command.command.clone();
    let command_text_for_edit = command.command.clone();

    // 获取菜单文本
    let execute_label = crate::i18n::t(&lang, "snippets.context_menu.execute");
    let edit_label = crate::i18n::t(&lang, "snippets.context_menu.edit_in_box");

    // 获取 PTY channel 用于执行命令
    let pty_channel = session_state
        .read(cx)
        .active_tab()
        .and_then(|tab| tab.pty_channel.clone());

    div()
        .id(SharedString::from(format!("cmd-{}", command_id)))
        .h(px(28.))
        .px_1()
        .flex()
        .items_center()
        .gap(px(2.))
        .rounded(px(4.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            // 点击复制命令到剪贴板
            cx.write_to_clipboard(ClipboardItem::new_string(command_text.clone()));
        })
        // 添加 tooltip 显示完整命令
        .tooltip(move |window, cx| Tooltip::new(command_text_for_tooltip.clone()).build(window, cx))
        // 添加右键菜单
        .context_menu(move |menu, _window, _cx| {
            let cmd_for_execute = command_text_for_execute.clone();
            let cmd_for_edit = command_text_for_edit.clone();
            let pty_for_menu = pty_channel.clone();
            let session_for_menu = session_state.clone();

            menu
                // 在终端执行
                .item({
                    let execute_label = execute_label.to_string();
                    PopupMenuItem::element(move |_window, cx| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(execute_label.clone())
                    })
                    .on_click(move |_, _window, cx| {
                        if let Some(channel) = &pty_for_menu {
                            let cmd = cmd_for_execute.clone();
                            let channel = Arc::clone(channel);
                            debug!("[ContextMenu] Executing command: {}", cmd);
                            cx.spawn(async move |_| {
                                let mut cmd_with_newline = cmd.into_bytes();
                                cmd_with_newline.push(0x0d); // CR
                                if let Err(e) = channel.write(&cmd_with_newline).await {
                                    tracing::error!(
                                        "[ContextMenu] Failed to send command: {:?}",
                                        e
                                    );
                                }
                            })
                            .detach();
                        }
                    })
                })
                // 在命令框编辑
                .item({
                    let edit_label = edit_label.to_string();
                    PopupMenuItem::element(move |_window, cx| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(edit_label.clone())
                    })
                    .on_click(move |_, window, cx| {
                        let cmd = cmd_for_edit.clone();
                        debug!("[ContextMenu] Edit in command box: {}", cmd);
                        session_for_menu.update(cx, |state, cx| {
                            state.set_command_input_text(cmd, window, cx);
                        });
                    })
                })
        })
        // 命令图标（紧贴左侧）
        .child(svg().path(icons::CODE).size(px(14.)).text_color(muted))
        // 命令名称
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(foreground)
                .overflow_hidden()
                .child(command.name.clone()),
        )
}
