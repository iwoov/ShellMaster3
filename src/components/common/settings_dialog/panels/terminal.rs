// 终端设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::menu::DropdownMenu;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::{
    BellStyle, CursorStyle, FontWeight as TerminalFontWeight, Osc52Policy,
};

use super::super::helpers::{
    render_font_input_row, render_input_row, render_number_row, render_section_title,
    render_switch_row, render_theme_select_row, TERMINAL_FONTS, TERMINAL_THEMES,
};
use super::super::SettingsDialogState;

/// 渲染终端设置面板
pub fn render_terminal_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let terminal = &state_read.settings.terminal;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let font_family_input = state_read.terminal_font_family_input.clone();
    let font_size_input = state_read.terminal_font_size_input.clone();
    let line_height_input = state_read.terminal_line_height_input.clone();
    let scrollback_input = state_read.scrollback_lines_input.clone();
    let blink_interval_input = state_read.terminal_blink_interval_input.clone();
    let opacity_input = state_read.terminal_background_opacity_input.clone();
    let padding_input = state_read.terminal_padding_input.clone();
    let scroll_multiplier_input = state_read.terminal_scroll_multiplier_input.clone();
    let foreground_input = state_read.terminal_foreground_input.clone();
    let background_input = state_read.terminal_background_input.clone();
    let cursor_color_input = state_read.terminal_cursor_color_input.clone();
    let selection_color_input = state_read.terminal_selection_color_input.clone();
    let word_separators_input = state_read.terminal_word_separators_input.clone();
    let term_type_input = state_read.terminal_term_type_input.clone();
    let default_shell_input = state_read.terminal_default_shell_input.clone();
    let shell_args_input = state_read.terminal_shell_args_input.clone();

    // 光标样式选项
    let cursor_style = terminal.cursor_style.clone();
    let cursor_style_label = match cursor_style {
        CursorStyle::Block => i18n::t(lang, "settings.terminal.cursor_style.block"),
        CursorStyle::Bar => i18n::t(lang, "settings.terminal.cursor_style.bar"),
        CursorStyle::Underline => i18n::t(lang, "settings.terminal.cursor_style.underline"),
    };

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 字体
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.font"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(font_family_input.as_ref().map(|input| {
                            render_font_input_row(
                                cx,
                                i18n::t(lang, "settings.terminal.font_family"),
                                input,
                                TERMINAL_FONTS,
                            )
                        }))
                        .children(font_size_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.font_size"),
                                input,
                                cx,
                            )
                        }))
                        .children(line_height_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.line_height"),
                                input,
                                cx,
                            )
                        }))
                        .child(render_choice_row(
                            "terminal-font-weight",
                            i18n::t(lang, "settings.terminal.font_weight"),
                            match &terminal.font_weight {
                                TerminalFontWeight::Normal => {
                                    i18n::t(lang, "settings.terminal.font_weight.normal")
                                }
                                TerminalFontWeight::Bold => {
                                    i18n::t(lang, "settings.terminal.font_weight.bold")
                                }
                            },
                            ChoiceKind::FontWeight,
                            state.clone(),
                            cx,
                        ))
                        .child(render_switch_row(
                            "terminal-ligatures",
                            i18n::t(lang, "settings.terminal.ligatures"),
                            terminal.ligatures,
                            state.clone(),
                            |s, v| s.settings.terminal.ligatures = v,
                            cx,
                        )),
                ),
        )
        // 配色
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.color_scheme"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_theme_select_row(
                            i18n::t(lang, "settings.terminal.theme"),
                            &terminal.color_scheme,
                            TERMINAL_THEMES,
                            state.clone(),
                            cx,
                        ))
                        .children(foreground_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.foreground_color"),
                                input,
                                cx,
                            )
                        }))
                        .children(background_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.background_color"),
                                input,
                                cx,
                            )
                        }))
                        .children(cursor_color_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.cursor_color"),
                                input,
                                cx,
                            )
                        }))
                        .children(selection_color_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.selection_color"),
                                input,
                                cx,
                            )
                        }))
                        .children(opacity_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.background_opacity"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
        // 显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.display"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        // 光标样式选择器
                        .child(render_cursor_style_row(
                            i18n::t(lang, "settings.terminal.cursor_style"),
                            cursor_style_label,
                            state.clone(),
                            cx,
                        ))
                        .child(render_switch_row(
                            "terminal-cursor-blink",
                            i18n::t(lang, "settings.terminal.cursor_blink"),
                            terminal.cursor_blink,
                            state.clone(),
                            |s, v| s.settings.terminal.cursor_blink = v,
                            cx,
                        ))
                        .children(blink_interval_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.cursor_blink_interval"),
                                input,
                                cx,
                            )
                        }))
                        .children(scrollback_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.scrollback"),
                                input,
                                cx,
                            )
                        }))
                        .children(padding_input.as_ref().map(|input| {
                            render_number_row(i18n::t(lang, "settings.terminal.padding"), input, cx)
                        }))
                        .child(render_switch_row(
                            "terminal-show-tab-bar",
                            i18n::t(lang, "settings.terminal.show_tab_bar"),
                            terminal.show_tab_bar,
                            state.clone(),
                            |s, v| s.settings.terminal.show_tab_bar = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "terminal-show-command-input",
                            i18n::t(lang, "settings.terminal.show_command_input"),
                            terminal.show_command_input,
                            state.clone(),
                            |s, v| s.settings.terminal.show_command_input = v,
                            cx,
                        )),
                ),
        )
        // 行为
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.behavior"),
                    cx,
                ))
                .child(render_switch_row(
                    "terminal-copy-on-select",
                    i18n::t(lang, "settings.terminal.copy_on_select"),
                    terminal.copy_on_select,
                    state.clone(),
                    |s, v| s.settings.terminal.copy_on_select = v,
                    cx,
                ))
                .child(render_switch_row(
                    "terminal-right-click-paste",
                    i18n::t(lang, "settings.terminal.right_click_paste"),
                    terminal.right_click_paste,
                    state.clone(),
                    |s, v| s.settings.terminal.right_click_paste = v,
                    cx,
                ))
                .child(render_switch_row(
                    "terminal-trim-copy",
                    i18n::t(lang, "settings.terminal.trim_trailing_whitespace"),
                    terminal.trim_trailing_whitespace,
                    state.clone(),
                    |s, v| s.settings.terminal.trim_trailing_whitespace = v,
                    cx,
                ))
                .child(render_switch_row(
                    "terminal-scroll-output",
                    i18n::t(lang, "settings.terminal.scroll_on_output"),
                    terminal.scroll_on_output,
                    state.clone(),
                    |s, v| s.settings.terminal.scroll_on_output = v,
                    cx,
                ))
                .children(scroll_multiplier_input.as_ref().map(|input| {
                    render_number_row(
                        i18n::t(lang, "settings.terminal.scroll_multiplier"),
                        input,
                        cx,
                    )
                }))
                .children(word_separators_input.as_ref().map(|input| {
                    render_input_row(
                        i18n::t(lang, "settings.terminal.word_separators"),
                        input,
                        cx,
                    )
                }))
                .child(render_choice_row(
                    "terminal-bell-style",
                    i18n::t(lang, "settings.terminal.bell_style"),
                    match &terminal.bell_style {
                        BellStyle::None => i18n::t(lang, "settings.terminal.bell.none"),
                        BellStyle::Visual => i18n::t(lang, "settings.terminal.bell.visual"),
                        BellStyle::Sound => i18n::t(lang, "settings.terminal.bell.sound"),
                    },
                    ChoiceKind::Bell,
                    state.clone(),
                    cx,
                )),
        )
        // 安全与兼容性
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.security_compatibility"),
                    cx,
                ))
                .child(render_choice_row(
                    "terminal-osc52-policy",
                    i18n::t(lang, "settings.terminal.osc52_policy"),
                    match &terminal.osc52_policy {
                        Osc52Policy::Disabled => i18n::t(lang, "settings.terminal.osc52.disabled"),
                        Osc52Policy::WriteOnly => {
                            i18n::t(lang, "settings.terminal.osc52.write_only")
                        }
                        Osc52Policy::ReadWrite => {
                            i18n::t(lang, "settings.terminal.osc52.read_write")
                        }
                    },
                    ChoiceKind::Osc52,
                    state.clone(),
                    cx,
                ))
                .children(term_type_input.as_ref().map(|input| {
                    render_input_row(i18n::t(lang, "settings.terminal.term_type"), input, cx)
                }))
                .children(default_shell_input.as_ref().map(|input| {
                    render_input_row(i18n::t(lang, "settings.terminal.default_shell"), input, cx)
                }))
                .children(shell_args_input.as_ref().map(|input| {
                    render_input_row(i18n::t(lang, "settings.terminal.shell_args"), input, cx)
                })),
        )
}

/// 渲染光标样式选择行
fn render_cursor_style_row(
    label: &'static str,
    current_label: &'static str,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;
    use gpui_component::menu::PopupMenuItem;

    let state_for_block = state.clone();
    let state_for_bar = state.clone();
    let state_for_underline = state.clone();

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            Button::new("cursor-style-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w(px(180.))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current_label),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    menu.min_w(px(200.))
                        .item(PopupMenuItem::new("Block").on_click({
                            let state = state_for_block.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Block;
                                    s.mark_changed();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Bar").on_click({
                            let state = state_for_bar.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Bar;
                                    s.mark_changed();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Underline").on_click({
                            let state = state_for_underline.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Underline;
                                    s.mark_changed();
                                });
                            }
                        }))
                }),
        )
}

#[derive(Clone, Copy)]
enum ChoiceKind {
    FontWeight,
    Bell,
    Osc52,
}

fn render_choice_row(
    id: &'static str,
    label: &'static str,
    current_label: &'static str,
    kind: ChoiceKind,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;
    use gpui_component::menu::PopupMenuItem;

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            Button::new(id)
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w(px(180.))
                        .child(div().text_sm().child(current_label))
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, cx| {
                    let lang = state.read(cx).settings.theme.language.clone();
                    let choices: Vec<(&'static str, u8)> = match kind {
                        ChoiceKind::FontWeight => vec![
                            (i18n::t(&lang, "settings.terminal.font_weight.normal"), 0),
                            (i18n::t(&lang, "settings.terminal.font_weight.bold"), 1),
                        ],
                        ChoiceKind::Bell => vec![
                            (i18n::t(&lang, "settings.terminal.bell.none"), 0),
                            (i18n::t(&lang, "settings.terminal.bell.visual"), 1),
                            (i18n::t(&lang, "settings.terminal.bell.sound"), 2),
                        ],
                        ChoiceKind::Osc52 => vec![
                            (i18n::t(&lang, "settings.terminal.osc52.disabled"), 0),
                            (i18n::t(&lang, "settings.terminal.osc52.write_only"), 1),
                            (i18n::t(&lang, "settings.terminal.osc52.read_write"), 2),
                        ],
                    };

                    choices
                        .into_iter()
                        .fold(menu.min_w(px(200.)), |menu, (choice_label, value)| {
                            let state = state.clone();
                            menu.item(PopupMenuItem::new(choice_label).on_click(move |_, _, cx| {
                                state.update(cx, |settings_state, _| {
                                    match kind {
                                        ChoiceKind::FontWeight => {
                                            settings_state.settings.terminal.font_weight =
                                                if value == 0 {
                                                    TerminalFontWeight::Normal
                                                } else {
                                                    TerminalFontWeight::Bold
                                                };
                                        }
                                        ChoiceKind::Bell => {
                                            settings_state.settings.terminal.bell_style =
                                                match value {
                                                    1 => BellStyle::Visual,
                                                    2 => BellStyle::Sound,
                                                    _ => BellStyle::None,
                                                };
                                        }
                                        ChoiceKind::Osc52 => {
                                            settings_state.settings.terminal.osc52_policy =
                                                match value {
                                                    0 => Osc52Policy::Disabled,
                                                    2 => Osc52Policy::ReadWrite,
                                                    _ => Osc52Policy::WriteOnly,
                                                };
                                        }
                                    }
                                    settings_state.mark_changed();
                                });
                            }))
                        })
                }),
        )
}
