// 辅助渲染函数和常量

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::color_picker::{ColorPicker, ColorPickerState};
use gpui_component::input::{Input, InputState, NumberInput, NumberInputEvent, StepAction};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme, Sizable};

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::terminal::rgb_to_hsla;

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
    // Nerd Fonts (powerlevel10k 推荐)
    "MesloLGS NF",
    "FiraCode Nerd Font",
    "JetBrainsMono Nerd Font",
    "Hack Nerd Font",
    // 普通等宽字体
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

pub fn render_section_title(title: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_base()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(title.into())
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

/// 渲染颜色选择行：色块 + hex 标签，点击弹出调色板（顶部为 `featured` 快捷色，可直接输入 hex）。
///
/// `current_hex` 用 settings 中保存的原始字符串，而不是 picker 内部的值：
/// picker 的 `to_hex` 按截断取整，直接显示会让未改动的颜色看起来变了。
pub fn render_color_picker_row(
    label: &'static str,
    picker: &Entity<ColorPickerState>,
    current_hex: &str,
    featured: Vec<Hsla>,
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
                .text_color(cx.theme().foreground)
                .child(label),
        )
        .child(
            div().w(px(200.)).child(
                // 样式作用在可点击的整行上，让它与同区域的输入框/下拉按钮观感一致。
                // small 尺寸的色块（20px + 4px 外边距）刚好放进 32px 高的框里。
                ColorPicker::new(picker)
                    .small()
                    .featured_colors(featured)
                    .label(current_hex.to_string())
                    .w_full()
                    .h(px(32.))
                    .border_1()
                    .border_color(cx.theme().input)
                    .text_color(cx.theme().foreground)
                    .cursor_pointer(),
            ),
        )
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
            div()
                .w(px(200.))
                .flex()
                .items_center()
                .gap_1()
                // 字体允许直接输入，兼容用户安装但不在推荐列表中的等宽字体。
                .child(div().flex_1().child(Input::new(input).appearance(true)))
                .child(
                    Button::new("font-dropdown")
                        .w(px(32.))
                        .h(px(32.))
                        .outline()
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        ))
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
                ),
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
                        menu = menu.item(PopupMenuItem::new(theme_name).on_click(
                            move |_, window, cx| {
                                let p = crate::terminal::palette_for(&theme_val);
                                let hex = |(r, g, b): (u8, u8, u8)| {
                                    format!("#{:02x}{:02x}{:02x}", r, g, b)
                                };
                                state_clone.update(cx, |s, cx| {
                                    s.settings.terminal.color_scheme = theme_val.clone();
                                    // 选择主题时同步前景/背景/光标/选择色
                                    s.settings.terminal.foreground_color = hex(p.foreground);
                                    s.settings.terminal.background_color = hex(p.background);
                                    s.settings.terminal.cursor_color = hex(p.cursor);
                                    s.settings.terminal.selection_color = hex(p.selection);
                                    s.mark_changed();
                                    // 预览卡片直接读取 settings，需要重绘
                                    cx.notify();
                                });

                                // 同步 4 个颜色选择器的色块。`set_value` 不会触发 Change 事件，
                                // settings 已在上面直接写入，所以不会重复写。
                                let pickers = {
                                    let state = state_clone.read(cx);
                                    [
                                        (state.terminal_foreground_picker.clone(), p.foreground),
                                        (state.terminal_background_picker.clone(), p.background),
                                        (state.terminal_cursor_picker.clone(), p.cursor),
                                        (state.terminal_selection_picker.clone(), p.selection),
                                    ]
                                };
                                for (picker, (r, g, b)) in pickers {
                                    if let Some(picker) = picker {
                                        picker.update(cx, |picker, cx| {
                                            picker.set_value(rgb_to_hsla(r, g, b), window, cx);
                                        });
                                    }
                                }
                            },
                        ));
                    }
                    menu
                }),
        )
}

/// 渲染下拉选择行：点击按钮弹出选项菜单，选中后通过 `update_fn` 写回设置
pub fn render_select_row<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    label: &'static str,
    current: &T,
    options: Vec<(T, &'static str)>,
    state: Entity<SettingsDialogState>,
    update_fn: fn(&mut SettingsDialogState, T),
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;

    let current_label = options
        .iter()
        .find(|(value, _)| value == current)
        .map(|(_, label)| *label)
        .unwrap_or_default();

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
                .text_color(cx.theme().foreground)
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
                    let mut menu = menu.min_w(px(200.));
                    for (value, option_label) in options.iter().cloned() {
                        let state = state.clone();
                        menu = menu.item(PopupMenuItem::new(option_label).on_click(
                            move |_, _, cx| {
                                let value = value.clone();
                                state.update(cx, |s, cx| {
                                    update_fn(s, value);
                                    s.mark_changed();
                                    cx.notify();
                                });
                            },
                        ));
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
                .text_color(cx.theme().foreground)
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

/// 创建带初始值和占位符的文本输入框（`masked` 为 true 时按密码显示）
pub fn create_text_input(
    initial_value: String,
    placeholder: &'static str,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<SettingsDialogState>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx)
            .placeholder(placeholder)
            .masked(masked);
        state.set_value(initial_value, window, cx);
        state
    })
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
