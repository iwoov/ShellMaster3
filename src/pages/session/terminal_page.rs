// Terminal 面板组件 - 包含终端区域和命令输入框

use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::state::SessionTab;
use crate::terminal::{hex_to_hsla, render_empty_terminal};

/// 渲染终端面板
pub fn render_terminal_panel(
    _tab: &SessionTab,
    command_input: Option<Entity<InputState>>,
    cx: &App,
) -> impl IntoElement {
    let border_color = cx.theme().border;

    // 获取语言设置和终端设置
    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let lang = settings.theme.language.clone();
    let terminal_settings = settings.terminal.clone();

    div()
        .size_full()
        .flex()
        .flex_col()
        // 终端显示区域（上方，占据大部分空间）
        .child(
            div()
                .id("terminal-display")
                .flex_1()
                .overflow_hidden()
                // TODO: 当有真正的终端实例时，使用 render_terminal_view
                // 目前显示空终端，使用设置中的背景色
                .child(render_empty_terminal(
                    &terminal_settings,
                    &i18n::t(&lang, "session.terminal.simulated"),
                    cx,
                )),
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
