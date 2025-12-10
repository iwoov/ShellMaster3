// 辅助渲染函数和常量

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState, NumberInput, NumberInputEvent, StepAction};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;

use super::SettingsDialogState;

// ======================== 常量 ========================

/// 常用界面字体
pub const UI_FONTS: &[&str] = &[
    "PingFang SC",
    "SF Pro",
    "Helvetica Neue",
    "Microsoft YaHei",
    "Source Han Sans SC",
    "Noto Sans SC",
    "Arial",
    "system-ui",
];

/// 常用终端等宽字体
pub const TERMINAL_FONTS: &[&str] = &[
    "JetBrains Mono",
    "Fira Code",
    "SF Mono",
    "Menlo",
    "Consolas",
    "Monaco",
    "Source Code Pro",
    "Hack",
    "IBM Plex Mono",
];

/// 常用终端主题
pub const TERMINAL_THEMES: &[&str] = &[
    "One Dark",
    "Dracula",
    "Solarized Dark",
    "Solarized Light",
    "Nord",
    "Monokai",
    "Gruvbox Dark",
    "Tokyo Night",
    "GitHub Dark",
];

// ======================== 辅助渲染函数 ========================

pub fn render_section_title(title: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_base()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(title)
}

/// 渲染带输入框的设置行（用于文本输入）
pub fn render_input_row(
    label: &'static str,
    input: &Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    let text_color = cx.theme().foreground;

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
                .text_color(text_color)
                .child(label),
        )
        .child(div().w(px(200.)).child(Input::new(input).appearance(true)))
}

/// 渲染字体输入框（带下拉选择按钮）
pub fn render_font_input_row(
    cx: &App,
    label: &'static str,
    input: &Entity<InputState>,
    fonts: &[&'static str],
) -> impl IntoElement {
    use gpui::Corner;

    let input_clone = input.clone();
    let current_value = input.read(cx).value().to_string();
    let fonts = fonts.to_vec();
    let fonts_clone = fonts.clone();

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
            // 使用全宽按钮作为下拉触发器，anchor 设为 TopLeft 以便菜单在正下方显示
            Button::new("font-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start() // 内容左对齐
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between() // 两端对齐：文字左侧，图标右侧
                        .w(px(180.)) // 明确设置宽度，减去按钮内边距
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current_value),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(200.));
                    for font in &fonts_clone {
                        let font_name: SharedString = (*font).into();
                        let input_for_click = input_clone.clone();
                        let font_val = font.to_string();
                        menu = menu.item(PopupMenuItem::new(font_name).on_click(
                            move |_, window, cx| {
                                input_for_click.update(cx, |state, cx| {
                                    state.set_value(font_val.clone(), window, cx);
                                });
                            },
                        ));
                    }
                    menu
                }),
        )
}

/// 渲染主题选择行（带下拉菜单）
pub fn render_theme_select_row(
    label: &'static str,
    current_value: &str,
    themes: &'static [&'static str],
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;

    let current = current_value.to_string();

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
            // 使用全宽按钮作为下拉触发器，anchor 设为 TopLeft 以便菜单在正下方显示
            Button::new("theme-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start() // 内容左对齐
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between() // 两端对齐：文字左侧，图标右侧
                        .w(px(180.)) // 明确设置宽度，减去按钮内边距
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(200.));
                    for theme in themes {
                        let theme_name: SharedString = (*theme).into();
                        let theme_val = theme.to_string();
                        let state_clone = state.clone();
                        menu =
                            menu.item(PopupMenuItem::new(theme_name).on_click(move |_, _, cx| {
                                state_clone.update(cx, |s, _| {
                                    s.settings.terminal.color_scheme = theme_val.clone();
                                    s.mark_changed();
                                });
                            }));
                    }
                    menu
                }),
        )
}

/// 渲染带数字输入框的设置行（带 +/- 按钮）
pub fn render_number_row(
    label: &'static str,
    input: &Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
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
            div()
                .w(px(200.))
                .flex()
                .justify_end() // 靠右对齐
                .child(NumberInput::new(input).appearance(true)),
        )
}

/// 渲染带开关的设置行
pub fn render_switch_row(
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    state: Entity<SettingsDialogState>,
    update_fn: fn(&mut SettingsDialogState, bool),
    cx: &App,
) -> impl IntoElement {
    let text_color = cx.theme().foreground;

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(div().text_sm().text_color(text_color).child(label))
        .child(
            Switch::new(id)
                .checked(checked)
                .on_click(move |new_val, _, cx| {
                    state.update(cx, |s, _| {
                        update_fn(s, *new_val);
                        s.mark_changed();
                    });
                }),
        )
}

pub fn render_about_row(label: &'static str, value: &'static str, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(value),
        )
}

/// 创建带有 +/- 按钮事件处理的整数输入框
pub fn create_int_number_input(
    initial_value: String,
    min: i32,
    max: i32,
    step: i32,
    window: &mut Window,
    cx: &mut Context<SettingsDialogState>,
) -> Entity<InputState> {
    let input = cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(initial_value, window, cx);
        state
    });
    cx.subscribe_in(&input, window, {
        move |_this, input, event: &NumberInputEvent, window, cx| match event {
            NumberInputEvent::Step(action) => input.update(cx, |input, cx| {
                if let Ok(value) = input.value().parse::<i32>() {
                    let new_value = if *action == StepAction::Increment {
                        (value + step).min(max)
                    } else {
                        (value - step).max(min)
                    };
                    input.set_value(new_value.to_string(), window, cx);
                }
            }),
        }
    })
    .detach();
    input
}

/// 创建带有 +/- 按钮事件处理的浮点数输入框
pub fn create_float_number_input(
    initial_value: String,
    min: f32,
    max: f32,
    step: f32,
    window: &mut Window,
    cx: &mut Context<SettingsDialogState>,
) -> Entity<InputState> {
    let input = cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(initial_value, window, cx);
        state
    });
    cx.subscribe_in(&input, window, {
        move |_this, input, event: &NumberInputEvent, window, cx| match event {
            NumberInputEvent::Step(action) => input.update(cx, |input, cx| {
                if let Ok(value) = input.value().parse::<f32>() {
                    let new_value = if *action == StepAction::Increment {
                        (value + step).min(max)
                    } else {
                        (value - step).max(min)
                    };
                    input.set_value(format!("{:.1}", new_value), window, cx);
                }
            }),
        }
    })
    .detach();
    input
}
