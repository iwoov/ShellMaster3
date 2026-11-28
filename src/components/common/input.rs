// 表单控件：输入框、下拉触发器、分段切换、键帽，以及表单字段和设置行的排版

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::Sizable;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::button::{CONTROL_HEIGHT, CONTROL_RADIUS};
use super::icon::icon_sized;
use crate::constants::icons;
use crate::theme::Palette;

/// 输入框、下拉框里文字的字号
pub const FIELD_TEXT_SIZE: f32 = 13.;

/// 30px 高的单行输入框。底色和边框取组件库主题，聚焦时边框变为主色。
///
/// 不要再给它设置边框色：组件库把实例样式放在最后，会盖掉聚焦边框。
/// 出错时调用 [`field_error`] 标红。
pub fn text_field(input: &Entity<InputState>) -> Input {
    let field = Input::new(input)
        .text_size(px(FIELD_TEXT_SIZE))
        .rounded(CONTROL_RADIUS)
        .py_0();
    // 单行输入框直接指定控件高度
    gpui_kit::Styled::h(field, CONTROL_HEIGHT)
}

/// 输入内容有误时把输入框边框标红（会替代聚焦边框）
pub fn field_error<T: Styled>(field: T, has_error: bool, p: &Palette) -> T {
    if has_error {
        field.border_color(p.danger)
    } else {
        field
    }
}

/// 多行输入框（高度由内容和 `auto_grow` 决定）
pub fn text_area(input: &Entity<TextareaState>) -> Textarea {
    Textarea::new(input)
        .text_size(px(FIELD_TEXT_SIZE))
        .rounded(CONTROL_RADIUS)
}

/// 下拉框用的组件库按钮样式：边框取 `field_border`，悬停和按下色与输入框一致
fn field_button_style(p: &Palette, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(p.field_border)
        .foreground(p.text)
        .hover(p.hover)
        .active(p.hover_strong)
}

/// 下拉框的触发按钮：外观与输入框一致，右侧带下拉箭头。
///
/// 菜单只能挂在组件库的 `Button` 上，所以这里返回 `Button`，
/// 调用方接着用 `dropdown_menu_with_anchor` 挂菜单。
pub fn select_trigger(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    width: Pixels,
    cx: &App,
) -> Button {
    let p = crate::theme::palette(cx);
    // small 尺寸的按钮左右各有 12px 内边距，再去掉两侧 1px 边框
    let inner_width = (width - px(26.)).max(px(0.));
    Button::new(id)
        .custom(field_button_style(&p, cx))
        .bg(p.field)
        .small()
        .rounded(CONTROL_RADIUS)
        .w(width)
        .h(CONTROL_HEIGHT)
        .justify_start()
        .child(
            div()
                .w(inner_width)
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(FIELD_TEXT_SIZE))
                        .text_color(p.text)
                        .child(label.into()),
                )
                .child(icon_sized(icons::CHEVRON_DOWN, 14., p.text_faint)),
        )
}

/// 只有下拉箭头的方形按钮（放在可输入的下拉框右侧，弹出推荐选项）
pub fn dropdown_icon_button(id: impl Into<ElementId>, cx: &App) -> Button {
    let p = crate::theme::palette(cx);
    Button::new(id)
        .custom(field_button_style(&p, cx))
        .bg(p.field)
        .small()
        .rounded(CONTROL_RADIUS)
        .w(CONTROL_HEIGHT)
        .h(CONTROL_HEIGHT)
        .child(icon_sized(icons::CHEVRON_DOWN, 14., p.text_faint))
}

/// 分段切换：几个互斥选项排成一排，选中项高亮。`fill` 为 true 时铺满宽度并均分。
pub fn segmented<T: Clone + PartialEq + 'static>(
    id: &str,
    options: Vec<(T, SharedString)>,
    selected: &T,
    fill: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + Clone + 'static,
    p: &Palette,
) -> Div {
    let selected_bg = if p.is_dark { p.hover_strong } else { p.surface };
    let (text, text_muted) = (p.text, p.text_muted);

    div()
        .flex_none()
        .when(fill, |track| track.w_full())
        .h(CONTROL_HEIGHT)
        .p(px(2.))
        .rounded(CONTROL_RADIUS)
        .bg(p.field)
        .border_1()
        .border_color(p.field_border)
        .flex()
        .items_center()
        .gap(px(2.))
        .children(
            options
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    let is_selected = &value == selected;
                    let on_select = on_select.clone();
                    div()
                        .id(SharedString::from(format!("{id}-{index}")))
                        .h_full()
                        .px(px(12.))
                        .when(fill, |item| item.flex_1())
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .whitespace_nowrap()
                        .text_size(px(FIELD_TEXT_SIZE))
                        .cursor_pointer()
                        .when(is_selected, |item| {
                            item.bg(selected_bg)
                                .text_color(text)
                                .font_weight(FontWeight::MEDIUM)
                                .when(!p.is_dark, |item| item.shadow_xs())
                        })
                        .when(!is_selected, |item| {
                            item.text_color(text_muted)
                                .hover(move |s| s.text_color(text))
                        })
                        .on_click(move |_, window, cx| {
                            if !is_selected {
                                on_select(value.clone(), window, cx);
                            }
                        })
                        .child(label)
                }),
        )
}

/// 键帽（快捷键里的一个键）
pub fn kbd(key: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .flex_none()
        .h(px(22.))
        .min_w(px(22.))
        .px(px(6.))
        .rounded(px(5.))
        .bg(p.field)
        .border_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text_muted)
        .child(key.into())
}

/// 把 "⌘Q"、"Ctrl+Shift+C" 这样的快捷键拆成单个键，用于逐个画键帽
pub fn shortcut_keys(shortcut: &str) -> Vec<String> {
    if shortcut.contains('+') {
        return shortcut
            .split('+')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .collect();
    }
    // macOS 写法：修饰键是单个符号，紧挨着主键
    const MODIFIERS: [char; 4] = ['⌘', '⌥', '⌃', '⇧'];
    let mut keys = Vec::new();
    let mut rest = shortcut;
    while let Some(first) = rest.chars().next() {
        if MODIFIERS.contains(&first) {
            keys.push(first.to_string());
            rest = &rest[first.len_utf8()..];
        } else {
            break;
        }
    }
    if !rest.is_empty() {
        keys.push(rest.to_string());
    }
    keys
}

/// 表单字段：标签在上、控件在下，出错时在下方显示错误
pub fn form_field(
    label: impl Into<SharedString>,
    control: impl IntoElement,
    error: Option<SharedString>,
    p: &Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.text_muted)
                .child(label.into()),
        )
        .child(control)
        .children(error.map(|error| div().text_size(px(12.)).text_color(p.danger).child(error)))
}

/// 设置分组卡片。每一行自带上边框作为分隔线，卡片的边框和裁剪会盖住第一行的那条。
pub fn settings_card(p: &Palette) -> Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .rounded(px(10.))
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .overflow_hidden()
}

/// 设置卡片里的一行：左边是标签和可选的说明，右边是控件
pub fn settings_row(
    label: impl Into<SharedString>,
    description: Option<SharedString>,
    control: impl IntoElement,
    p: &Palette,
) -> Div {
    div()
        // 上移 1px，让分隔线与上一行重叠；第一行的分隔线会被卡片边框裁掉
        .mt(px(-1.))
        .border_t_1()
        .border_color(p.line)
        .min_h(px(48.))
        .px(px(14.))
        .py(px(9.))
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.text)
                        .child(label.into()),
                )
                .children(description.map(|description| {
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(description)
                })),
        )
        .child(div().flex_none().flex().items_center().child(control))
}

#[cfg(test)]
mod tests {
    use super::shortcut_keys;

    #[test]
    fn shortcuts_split_into_keys() {
        assert_eq!(shortcut_keys("⌘Q"), vec!["⌘", "Q"]);
        assert_eq!(shortcut_keys("⌘⇧C"), vec!["⌘", "⇧", "C"]);
        assert_eq!(shortcut_keys("Ctrl+Shift+C"), vec!["Ctrl", "Shift", "C"]);
        assert_eq!(shortcut_keys("Ctrl + Q"), vec!["Ctrl", "Q"]);
        assert_eq!(shortcut_keys("Esc"), vec!["Esc"]);
    }
}
