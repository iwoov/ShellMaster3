// Terminal 面板组件 - 包含终端区域和命令输入框

use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::state::SessionTab;

/// 渲染终端面板
pub fn render_terminal_panel(
    _tab: &SessionTab,
    command_input: Option<Entity<InputState>>,
    cx: &App,
) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        // 模拟终端区域（上方，占据大部分空间）
        .child(
            div()
                .id("terminal-display")
                .flex_1()
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(i18n::t(&lang, "session.terminal.simulated")),
                ),
        )
        // 命令输入区域（下方）
        .child(render_command_input(border_color, command_input, cx))
}

/// 渲染命令输入区域
fn render_command_input(
    border_color: Hsla,
    command_input: Option<Entity<InputState>>,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;

    div()
        .id("command-input-area")
        .flex_shrink_0()
        .border_t_1()
        .border_color(border_color)
        .p_1()
        .child(
            // 输入框容器
            div()
                .w_full()
                .flex()
                .items_end()
                .gap_1()
                // 输入框
                .child(
                    div().flex_1().children(
                        command_input
                            .as_ref()
                            .map(|input| Input::new(input).appearance(false)),
                    ),
                )
                // 发送按钮
                .child(
                    div()
                        .id("send-command-btn")
                        .size(px(24.))
                        .rounded(px(4.))
                        .bg(primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(primary.opacity(0.8)))
                        .child(
                            svg()
                                .path(icons::SEND)
                                .size(px(14.))
                                .text_color(gpui::white()),
                        ),
                ),
        )
}
