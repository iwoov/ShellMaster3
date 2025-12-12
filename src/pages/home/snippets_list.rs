// Snippets (快捷命令) 列表页面组件

use gpui::*;
use gpui_component::ActiveTheme;
use tracing::error;

use crate::components::common::icon::render_icon;
use crate::components::common::snippets_dialog::SnippetsDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{SnippetCommand, SnippetGroup, SnippetsConfig};
use crate::services::storage;

/// Snippets 页面状态（仅包含页面导航状态）
pub struct SnippetsPageState {
    /// 当前路径（命令组 ID 栈，空表示根目录）
    pub current_path: Vec<String>,
    /// 当前加载的配置
    pub config: SnippetsConfig,
    /// 刷新标记
    pub needs_refresh: bool,
    /// 弹窗状态
    pub dialog_state: Entity<SnippetsDialogState>,
}

impl SnippetsPageState {
    pub fn new(cx: &mut App) -> Self {
        let config = storage::load_snippets().unwrap_or_default();
        let dialog_state = cx.new(|_| SnippetsDialogState::default());
        Self {
            current_path: vec![],
            config,
            needs_refresh: false,
            dialog_state,
        }
    }

    /// 刷新配置
    pub fn refresh(&mut self) {
        self.config = storage::load_snippets().unwrap_or_default();
        self.needs_refresh = false;
    }

    /// 进入子组
    pub fn enter_group(&mut self, group_id: String) {
        self.current_path.push(group_id);
    }

    /// 返回上级 (点击面包屑)
    pub fn go_to_level(&mut self, level: usize) {
        self.current_path.truncate(level);
    }

    /// 获取当前父级 ID
    pub fn current_parent_id(&self) -> Option<&str> {
        self.current_path.last().map(|s| s.as_str())
    }

    /// 获取当前层级的组
    pub fn current_groups(&self) -> Vec<&SnippetGroup> {
        self.config.get_child_groups(self.current_parent_id())
    }

    /// 获取当前层级的命令
    pub fn current_commands(&self) -> Vec<&SnippetCommand> {
        self.config.get_commands_in_group(self.current_parent_id())
    }

    /// 打开添加组弹窗
    pub fn open_add_group(&mut self, cx: &mut App) {
        let parent_id = self.current_parent_id().map(|s| s.to_string());
        self.dialog_state.update(cx, |s, _| {
            s.set_parent_id(parent_id);
            s.open_add_group();
        });
    }

    /// 打开编辑组弹窗
    pub fn open_edit_group(&mut self, group: &SnippetGroup, cx: &mut App) {
        self.dialog_state.update(cx, |s, _| {
            s.open_edit_group(group);
        });
    }

    /// 打开添加命令弹窗
    pub fn open_add_command(&mut self, cx: &mut App) {
        let parent_id = self.current_parent_id().map(|s| s.to_string());
        self.dialog_state.update(cx, |s, _| {
            s.set_parent_id(parent_id);
            s.open_add_command();
        });
    }

    /// 打开编辑命令弹窗
    pub fn open_edit_command(&mut self, command: &SnippetCommand, cx: &mut App) {
        self.dialog_state.update(cx, |s, _| {
            s.open_edit_command(command);
        });
    }

    /// 检查弹窗是否打开
    #[allow(dead_code)]
    pub fn is_dialog_open(&self, cx: &App) -> bool {
        self.dialog_state.read(cx).is_open()
    }
}

/// 卡片颜色配置
#[derive(Clone, Copy)]
struct CardColors {
    bg: Hsla,
    border: Hsla,
    primary: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    secondary_hover: Hsla,
    destructive: Hsla,
}

/// 渲染 Snippets 内容区域
pub fn render_snippets_content(state: Entity<SnippetsPageState>, cx: &App) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        secondary_hover: cx.theme().secondary_hover,
        destructive: rgb(0xef4444).into(),
    };

    let state_read = state.read(cx);
    let current_path = state_read.current_path.clone();
    let groups: Vec<SnippetGroup> = state_read
        .current_groups()
        .iter()
        .map(|g| (*g).clone())
        .collect();
    let commands: Vec<SnippetCommand> = state_read
        .current_commands()
        .iter()
        .map(|c| (*c).clone())
        .collect();
    let config = state_read.config.clone();
    let has_items = !groups.is_empty() || !commands.is_empty();

    let bg_color = crate::theme::background_color(cx);

    div()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .bg(bg_color)
        .flex()
        .flex_col()
        .relative()
        // 工具栏
        .child(render_toolbar(state.clone(), &lang, colors, cx))
        // 面包屑导航
        .child(render_breadcrumb(
            state.clone(),
            &current_path,
            &config,
            &lang,
            colors,
            cx,
        ))
        // 卡片内容区域
        .child(if has_items {
            div()
                .id("snippets-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px_6()
                .pb_6()
                .child(render_card_grid(
                    state.clone(),
                    groups,
                    commands,
                    &config,
                    colors,
                    cx,
                ))
                .into_any_element()
        } else {
            render_empty_state(state.clone(), &lang, colors, cx).into_any_element()
        })
    // 弹窗在 page.rs 的 render_home_view 中统一渲染
}

/// 渲染工具栏
fn render_toolbar(
    state: Entity<SnippetsPageState>,
    lang: &Language,
    colors: CardColors,
    cx: &App,
) -> impl IntoElement {
    let state_for_group = state.clone();
    let state_for_command = state;

    div()
        .flex_shrink_0()
        .p_6()
        .pb_4()
        .flex()
        .gap_3()
        .child(
            // 新建组按钮
            div()
                .id("add-group-btn")
                .px_4()
                .py_2()
                .bg(cx.theme().secondary)
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(cx.theme().secondary_hover))
                .flex()
                .items_center()
                .gap_2()
                .on_click(move |_, _, cx| {
                    state_for_group.update(cx, |s, cx| s.open_add_group(cx));
                })
                .child(render_icon(icons::FOLDER, colors.muted_foreground.into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(colors.foreground)
                        .child(i18n::t(lang, "snippets.add_group")),
                ),
        )
        .child(
            // 新建命令按钮
            div()
                .id("add-command-btn")
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
                    state_for_command.update(cx, |s, cx| s.open_add_command(cx));
                })
                .child(render_icon(icons::PLUS, rgb(0xffffff).into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().primary_foreground)
                        .child(i18n::t(lang, "snippets.add_command")),
                ),
        )
}

/// 渲染面包屑导航
fn render_breadcrumb(
    state: Entity<SnippetsPageState>,
    current_path: &[String],
    config: &SnippetsConfig,
    lang: &Language,
    colors: CardColors,
    _cx: &App,
) -> impl IntoElement {
    let breadcrumb = config.build_breadcrumb(current_path);
    let state_for_root = state.clone();
    let is_root = current_path.is_empty();

    div()
        .px_6()
        .pb_2()
        .flex()
        .items_center()
        .gap_1()
        // 根目录 "全部"
        .child(
            div()
                .id("breadcrumb-root")
                .text_sm()
                .text_color(if is_root {
                    colors.foreground
                } else {
                    colors.primary
                })
                .cursor(if is_root {
                    CursorStyle::Arrow
                } else {
                    CursorStyle::PointingHand
                })
                .hover(move |s| if !is_root { s.underline() } else { s })
                .on_click(move |_, _, cx| {
                    state_for_root.update(cx, |s, _| s.go_to_level(0));
                })
                .child(i18n::t(lang, "snippets.breadcrumb.all")),
        )
        // 路径分隔符和层级
        .children(breadcrumb.iter().enumerate().map(|(i, (id, name))| {
            let is_last = i == breadcrumb.len() - 1;
            let state_for_click = state.clone();
            let level = i + 1;
            let name_owned = name.clone();
            let id_owned = id.clone();

            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(colors.muted_foreground)
                        .child(" > "),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("breadcrumb-{}", id_owned)))
                        .text_sm()
                        .text_color(if is_last {
                            colors.foreground
                        } else {
                            colors.primary
                        })
                        .cursor(if is_last {
                            CursorStyle::Arrow
                        } else {
                            CursorStyle::PointingHand
                        })
                        .hover(move |s| if is_last { s } else { s.underline() })
                        .on_click(move |_, _, cx| {
                            if !is_last {
                                state_for_click.update(cx, |s, _| s.go_to_level(level));
                            }
                        })
                        .child(name_owned),
                )
        }))
}

/// 渲染卡片网格
fn render_card_grid(
    state: Entity<SnippetsPageState>,
    groups: Vec<SnippetGroup>,
    commands: Vec<SnippetCommand>,
    config: &SnippetsConfig,
    colors: CardColors,
    _cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_4()
        // 命令组卡片
        .children(groups.into_iter().map(|group| {
            let state_clone = state.clone();
            let child_count = config.count_children(&group.id);
            render_group_card(state_clone, group, child_count, colors)
        }))
        // 命令卡片
        .children(commands.into_iter().map(|command| {
            let state_clone = state.clone();
            render_command_card(state_clone, command, colors)
        }))
}

/// 渲染命令组卡片
fn render_group_card(
    state: Entity<SnippetsPageState>,
    group: SnippetGroup,
    child_count: usize,
    colors: CardColors,
) -> impl IntoElement {
    let group_id = group.id.clone();
    let group_for_enter = group_id.clone();
    let group_for_edit = group.clone();
    let group_id_for_delete = group_id.clone();
    let state_for_enter = state.clone();
    let state_for_edit = state.clone();
    let state_for_delete = state;

    div()
        .id(SharedString::from(format!("group-card-{}", group_id)))
        .w(px(180.))
        .h(px(120.))
        .bg(colors.bg)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .p_4()
        .cursor_pointer()
        .hover(move |s| s.border_color(colors.primary).shadow_md())
        .on_click(move |_, _, cx| {
            state_for_enter.update(cx, |s, _| s.enter_group(group_for_enter.clone()));
        })
        .flex()
        .flex_col()
        .justify_between()
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
                        .child(render_icon(icons::FOLDER, colors.primary.into())),
                )
                .child(
                    div().flex_1().overflow_hidden().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .overflow_hidden()
                            .child(group.name.clone()),
                    ),
                ),
        )
        .child(
            // 底部：子项数量和操作按钮
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(format!("{} 项", child_count)),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "group-edit-{}",
                                    group_id.clone()
                                )))
                                .cursor_pointer()
                                .p_1()
                                .rounded_sm()
                                .hover(move |s| s.bg(colors.secondary_hover))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    state_for_edit
                                        .update(cx, |s, cx| s.open_edit_group(&group_for_edit, cx));
                                })
                                .child(render_icon(icons::EDIT, colors.muted_foreground.into())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("group-delete-{}", group_id)))
                                .cursor_pointer()
                                .p_1()
                                .rounded_sm()
                                .hover(move |s| s.bg(colors.destructive.opacity(0.1)))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    if let Err(e) =
                                        storage::delete_snippet_group(&group_id_for_delete)
                                    {
                                        error!("Failed to delete snippet group: {}", e);
                                    }
                                    state_for_delete.update(cx, |s, _| {
                                        s.needs_refresh = true;
                                        s.refresh();
                                    });
                                })
                                .child(render_icon(icons::TRASH, colors.destructive.into())),
                        ),
                ),
        )
}

/// 渲染命令卡片
fn render_command_card(
    state: Entity<SnippetsPageState>,
    command: SnippetCommand,
    colors: CardColors,
) -> impl IntoElement {
    let command_id = command.id.clone();
    let command_text = command.command.clone();
    let command_for_edit = command.clone();
    let command_id_for_delete = command_id.clone();
    let state_for_edit = state.clone();
    let state_for_delete = state;

    div()
        .id(SharedString::from(format!("command-card-{}", command_id)))
        .w(px(180.))
        .h(px(120.))
        .bg(colors.bg)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .p_4()
        .cursor_pointer()
        .hover(move |s| s.border_color(colors.primary).shadow_md())
        // 点击复制命令
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(command_text.clone()));
        })
        .flex()
        .flex_col()
        .justify_between()
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
                        .bg(colors.muted_foreground.opacity(0.1))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icons::CODE, colors.muted_foreground.into())),
                )
                .child(
                    div().flex_1().overflow_hidden().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .overflow_hidden()
                            .child(command.name.clone()),
                    ),
                ),
        )
        .child(
            // 中部：命令预览
            div()
                .text_xs()
                .text_color(colors.muted_foreground)
                .overflow_hidden()
                .child(truncate_command(&command.command, 30)),
        )
        .child(
            // 底部：操作按钮
            div().flex().justify_end().items_center().child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "command-edit-{}",
                                command_id.clone()
                            )))
                            .cursor_pointer()
                            .p_1()
                            .rounded_sm()
                            .hover(move |s| s.bg(colors.secondary_hover))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                cx.stop_propagation();
                                state_for_edit
                                    .update(cx, |s, cx| s.open_edit_command(&command_for_edit, cx));
                            })
                            .child(render_icon(icons::EDIT, colors.muted_foreground.into())),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("command-delete-{}", command_id)))
                            .cursor_pointer()
                            .p_1()
                            .rounded_sm()
                            .hover(move |s| s.bg(colors.destructive.opacity(0.1)))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                cx.stop_propagation();
                                if let Err(e) =
                                    storage::delete_snippet_command(&command_id_for_delete)
                                {
                                    error!("Failed to delete snippet command: {}", e);
                                }
                                state_for_delete.update(cx, |s, _| {
                                    s.needs_refresh = true;
                                    s.refresh();
                                });
                            })
                            .child(render_icon(icons::TRASH, colors.destructive.into())),
                    ),
            ),
        )
}

/// 渲染空状态
fn render_empty_state(
    state: Entity<SnippetsPageState>,
    lang: &Language,
    colors: CardColors,
    cx: &App,
) -> impl IntoElement {
    let state_for_group = state.clone();
    let state_for_command = state;

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
                .rounded_full()
                .bg(colors.primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::CODE, colors.primary.into())),
        )
        .child(
            div()
                .text_lg()
                .text_color(colors.foreground)
                .child(i18n::t(lang, "snippets.empty.title")),
        )
        .child(
            div()
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(i18n::t(lang, "snippets.empty.description")),
        )
        .child(
            div()
                .flex()
                .gap_3()
                .pt_2()
                .child(
                    div()
                        .id("empty-add-group-btn")
                        .px_4()
                        .py_2()
                        .bg(cx.theme().secondary)
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |s| s.bg(cx.theme().secondary_hover))
                        .flex()
                        .items_center()
                        .gap_2()
                        .on_click(move |_, _, cx| {
                            state_for_group.update(cx, |s, cx| s.open_add_group(cx));
                        })
                        .child(render_icon(icons::FOLDER, colors.muted_foreground.into()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(colors.foreground)
                                .child(i18n::t(lang, "snippets.add_group")),
                        ),
                )
                .child(
                    div()
                        .id("empty-add-command-btn")
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
                            state_for_command.update(cx, |s, cx| s.open_add_command(cx));
                        })
                        .child(render_icon(icons::PLUS, rgb(0xffffff).into()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().primary_foreground)
                                .child(i18n::t(lang, "snippets.add_command")),
                        ),
                ),
        )
}

/// 截断命令显示
fn truncate_command(cmd: &str, max_len: usize) -> String {
    if cmd.len() <= max_len {
        cmd.to_string()
    } else {
        format!("{}...", &cmd[..max_len])
    }
}
