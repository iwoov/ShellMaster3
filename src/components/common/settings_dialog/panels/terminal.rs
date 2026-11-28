// 终端设置面板

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

use crate::components::common::input::select_trigger;
use crate::i18n;
use crate::models::settings::{
    BellStyle, CursorStyle, FontWeight as TerminalFontWeight, Osc52Policy,
};
use crate::terminal::{palette_for, rgb_to_hsla};

use super::super::helpers::{
    render_block_row, render_card, render_color_picker_row, render_font_input_row,
    render_input_row, render_number_row, render_row, render_section_title, render_switch_row,
    render_theme_select_row, CONTROL_WIDTH, TERMINAL_FONTS, TERMINAL_THEMES,
};
use super::super::SettingsDialogState;
use super::terminal_preview::render_terminal_preview;

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
    let foreground_picker = state_read.terminal_foreground_picker.clone();
    let background_picker = state_read.terminal_background_picker.clone();
    let cursor_picker = state_read.terminal_cursor_picker.clone();
    let selection_picker = state_read.terminal_selection_picker.clone();
    let word_separators_input = state_read.terminal_word_separators_input.clone();
    let term_type_input = state_read.terminal_term_type_input.clone();
    let default_shell_input = state_read.terminal_default_shell_input.clone();
    let shell_args_input = state_read.terminal_shell_args_input.clone();

    // 调色板弹窗顶部的快捷色
    let featured_colors = theme_featured_colors(&terminal.color_scheme);

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
                    render_card(cx)
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
                    render_card(cx)
                        .child(render_theme_select_row(
                            i18n::t(lang, "settings.terminal.theme"),
                            &terminal.color_scheme,
                            TERMINAL_THEMES,
                            state.clone(),
                            cx,
                        ))
                        // 模拟终端预览：实时反映下面的颜色、上面的字体，以及显示区的光标样式
                        .child(render_block_row(
                            render_terminal_preview(state_read, cx),
                            cx,
                        ))
                        .children(foreground_picker.as_ref().map(|picker| {
                            render_color_picker_row(
                                i18n::t(lang, "settings.terminal.foreground_color"),
                                picker,
                                &terminal.foreground_color,
                                featured_colors.clone(),
                                cx,
                            )
                        }))
                        .children(background_picker.as_ref().map(|picker| {
                            render_color_picker_row(
                                i18n::t(lang, "settings.terminal.background_color"),
                                picker,
                                &terminal.background_color,
                                featured_colors.clone(),
                                cx,
                            )
                        }))
                        .children(cursor_picker.as_ref().map(|picker| {
                            render_color_picker_row(
                                i18n::t(lang, "settings.terminal.cursor_color"),
                                picker,
                                &terminal.cursor_color,
                                featured_colors.clone(),
                                cx,
                            )
                        }))
                        .children(selection_picker.as_ref().map(|picker| {
                            render_color_picker_row(
                                i18n::t(lang, "settings.terminal.selection_color"),
                                picker,
                                &terminal.selection_color,
                                featured_colors.clone(),
                                cx,
                            )
                        }))
                        .children(opacity_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.background_opacity"),
                                input,
                                cx,
                            )
                        }))
                        .child(render_switch_row(
                            "terminal-keyword-highlight",
                            i18n::t(lang, "settings.terminal.keyword_highlight"),
                            terminal.keyword_highlight,
                            state.clone(),
                            |s, v| s.settings.terminal.keyword_highlight = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "terminal-auto-color-ls",
                            i18n::t(lang, "settings.terminal.auto_color_ls"),
                            terminal.auto_color_ls,
                            state.clone(),
                            |s, v| s.settings.terminal.auto_color_ls = v,
                            cx,
                        )),
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
                    render_card(cx)
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
                .child(
                    render_card(cx)
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
                ),
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
                .child(
                    render_card(cx)
                        .child(render_choice_row(
                            "terminal-osc52-policy",
                            i18n::t(lang, "settings.terminal.osc52_policy"),
                            match &terminal.osc52_policy {
                                Osc52Policy::Disabled => {
                                    i18n::t(lang, "settings.terminal.osc52.disabled")
                                }
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
                            render_input_row(
                                i18n::t(lang, "settings.terminal.term_type"),
                                input,
                                cx,
                            )
                        }))
                        .children(default_shell_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.default_shell"),
                                input,
                                cx,
                            )
                        }))
                        .children(shell_args_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.terminal.shell_args"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
}

/// 调色板弹窗顶部的快捷色：当前主题的 背景/前景/光标/选区 + ANSI 红绿黄蓝品青（最多 10 个）。
fn theme_featured_colors(scheme: &str) -> Vec<Hsla> {
    let palette = palette_for(scheme);
    let mut colors: Vec<(u8, u8, u8)> = Vec::with_capacity(10);
    for rgb in [
        palette.background,
        palette.foreground,
        palette.cursor,
        palette.selection,
    ]
    .into_iter()
    .chain(palette.ansi[1..=6].iter().copied())
    {
        // 部分主题的光标色与前景色相同：重复的色块没有意义，还会让弹窗里出现 id 相同的元素
        if !colors.contains(&rgb) {
            colors.push(rgb);
        }
    }
    colors
        .into_iter()
        .map(|(r, g, b)| rgb_to_hsla(r, g, b))
        .collect()
}

/// 渲染光标样式选择行
fn render_cursor_style_row(
    label: &'static str,
    current_label: &'static str,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    render_row(
        label,
        None,
        select_trigger("cursor-style-dropdown", current_label, CONTROL_WIDTH, cx)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, cx| {
                let lang = state.read(cx).settings.theme.language.clone();
                [
                    (CursorStyle::Block, "settings.terminal.cursor_style.block"),
                    (CursorStyle::Bar, "settings.terminal.cursor_style.bar"),
                    (
                        CursorStyle::Underline,
                        "settings.terminal.cursor_style.underline",
                    ),
                ]
                .into_iter()
                .fold(menu.min_w(CONTROL_WIDTH), |menu, (style, key)| {
                    let state = state.clone();
                    menu.item(
                        PopupMenuItem::new(i18n::t(&lang, key)).on_click(move |_, _, cx| {
                            let style = style.clone();
                            state.update(cx, |s, cx| {
                                s.settings.terminal.cursor_style = style;
                                s.mark_changed();
                                cx.notify();
                            });
                        }),
                    )
                })
            }),
        cx,
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
    render_row(
        label,
        None,
        select_trigger(id, current_label, CONTROL_WIDTH, cx).dropdown_menu_with_anchor(
            Anchor::TopLeft,
            move |menu, _, cx| {
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

                choices.into_iter().fold(
                    menu.min_w(CONTROL_WIDTH),
                    |menu, (choice_label, value)| {
                        let state = state.clone();
                        menu.item(PopupMenuItem::new(choice_label).on_click(move |_, _, cx| {
                            state.update(cx, |settings_state, cx| {
                                match kind {
                                    ChoiceKind::FontWeight => {
                                        settings_state.settings.terminal.font_weight = if value == 0
                                        {
                                            TerminalFontWeight::Normal
                                        } else {
                                            TerminalFontWeight::Bold
                                        };
                                    }
                                    ChoiceKind::Bell => {
                                        settings_state.settings.terminal.bell_style = match value {
                                            1 => BellStyle::Visual,
                                            2 => BellStyle::Sound,
                                            _ => BellStyle::None,
                                        };
                                    }
                                    ChoiceKind::Osc52 => {
                                        settings_state.settings.terminal.osc52_policy = match value
                                        {
                                            0 => Osc52Policy::Disabled,
                                            2 => Osc52Policy::ReadWrite,
                                            _ => Osc52Policy::WriteOnly,
                                        };
                                    }
                                }
                                settings_state.mark_changed();
                                cx.notify();
                            });
                        }))
                    },
                )
            },
        ),
        cx,
    )
}

#[cfg(test)]
mod tests {
    // 不要写 `use super::*`：父模块里的 `use gpui_kit::*` 会把 `#[gpui_kit::test]` 一并带进来，
    // 盖过内置的 `#[test]`，导致宏展开递归超限。
    use super::super::super::helpers::TERMINAL_THEMES;
    use super::theme_featured_colors;
    use crate::terminal::{hsla_to_hex, palette_for};

    #[test]
    fn featured_colors_are_unique_and_start_with_the_background() {
        for scheme in TERMINAL_THEMES {
            let hexes: Vec<String> = theme_featured_colors(scheme)
                .into_iter()
                .map(hsla_to_hex)
                .collect();

            let mut unique = hexes.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(
                hexes.len(),
                unique.len(),
                "{scheme}: duplicates in {hexes:?}"
            );
            // 背景、前景、光标、选区 + 6 个 ANSI 色；光标与前景同色的主题少 1 个
            assert!(
                (9..=10).contains(&hexes.len()),
                "{scheme}: unexpected count {}",
                hexes.len()
            );

            let (r, g, b) = palette_for(scheme).background;
            assert_eq!(hexes[0], format!("#{r:02x}{g:02x}{b:02x}"), "{scheme}");
        }
    }
}
