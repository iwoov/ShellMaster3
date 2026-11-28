// 服务器弹窗的表单辅助函数

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::input::{Input, InputState};

use crate::components::common::button::CONTROL_HEIGHT;
use crate::i18n;
use crate::models::settings::Language;
use crate::theme::Palette;

use super::{Field, FormErrors};

/// 字段的错误提示（没有错误时为 None）
pub fn error_text(errors: &FormErrors, field: Field, lang: &Language) -> Option<SharedString> {
    errors
        .iter()
        .find(|(error_field, _)| *error_field == field)
        .map(|(_, key)| SharedString::from(i18n::t(lang, key)))
}

/// 输入框；还没创建时（打开后的第一帧之前）先占位，避免布局跳动
pub fn input_or_placeholder(
    input: Option<&Entity<InputState>>,
    build: impl FnOnce(&Entity<InputState>) -> Input,
) -> AnyElement {
    match input {
        Some(input) => build(input).into_any_element(),
        None => div().h(CONTROL_HEIGHT).into_any_element(),
    }
}

/// 字段下方的错误提示
pub fn error_line(text: SharedString, p: &Palette) -> Div {
    div().text_size(px(12.)).text_color(p.danger).child(text)
}

/// 两个字段并排（主机地址 + 端口）：左边占满剩余宽度，右边固定宽度。
/// 两个字段的错误提示统一显示在这一行下方，避免在窄的一栏里折成好几行。
pub fn field_pair(
    left: Div,
    right: Div,
    right_width: Pixels,
    errors: impl IntoIterator<Item = SharedString>,
    p: &Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(left.flex_1().min_w_0())
                .child(right.flex_none().w(right_width)),
        )
        .children(errors.into_iter().map(|error| error_line(error, p)))
}

/// 提示框（说明、警告等）：文字、底色和边框都取 `color` 的色调
pub fn notice(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .px(px(12.))
        .py(px(9.))
        .rounded(px(8.))
        .bg(color.opacity(0.08))
        .border_1()
        .border_color(color.opacity(0.3))
        .text_size(px(12.))
        .text_color(color)
        .child(text.into())
}

/// 表单下方的说明文字
pub fn hint(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .text_size(px(12.))
        .text_color(p.text_muted)
        .child(text.into())
}

/// 单选圆点
pub fn radio(selected: bool, p: &Palette) -> Div {
    div()
        .flex_none()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(if selected { p.primary } else { p.text_faint })
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |dot| {
            dot.child(div().size(px(8.)).rounded_full().bg(p.primary))
        })
}
