// 按钮组件：与主页一致的 30px 按钮和 28px 图标按钮

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::tooltip::Tooltip;

use crate::components::common::icon::icon_sized;
use crate::theme::Palette;

/// 按钮、输入框、下拉框等控件的统一高度
pub const CONTROL_HEIGHT: Pixels = px(30.);
/// 按钮、输入框、下拉框等控件的统一圆角
pub const CONTROL_RADIUS: Pixels = px(7.);
/// 图标按钮的边长
pub const ICON_BUTTON_SIZE: Pixels = px(28.);

/// 图标按钮内部用于联动悬停样式的分组名
const ICON_BUTTON_GROUP: &str = "icon-button";

/// 按钮的外观
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    /// 实心强调色：每个界面唯一的主要操作
    Primary,
    /// 描边：与主按钮并排的次要操作
    Secondary,
    /// 实心红色：删除等不可撤销的操作
    Danger,
}

/// 通用按钮。`enabled` 为 false 时变淡且没有悬停效果，调用方不要再绑定点击。
pub fn button(
    id: impl Into<ElementId>,
    kind: ButtonKind,
    icon: Option<&'static str>,
    label: impl Into<SharedString>,
    enabled: bool,
    p: &Palette,
) -> Stateful<Div> {
    let white: Hsla = gpui::white();
    let (bg, hover_bg, border, text, icon_color) = match kind {
        ButtonKind::Primary => (
            p.primary,
            p.primary_hover,
            None,
            p.primary_foreground,
            p.primary_foreground,
        ),
        ButtonKind::Secondary => (
            p.surface,
            p.hover_strong,
            Some(p.border),
            p.text,
            p.text_muted,
        ),
        ButtonKind::Danger => (p.danger, p.danger_hover, None, white, white),
    };

    div()
        .id(id)
        .flex_none()
        .h(CONTROL_HEIGHT)
        .map(|button| match icon {
            Some(_) => button.pl(px(9.)).pr(px(12.)),
            None => button.px(px(14.)),
        })
        .rounded(CONTROL_RADIUS)
        .bg(bg)
        .when_some(border, |button, color| {
            button.border_1().border_color(color)
        })
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.))
        .when(enabled, |button| {
            button.cursor_pointer().hover(move |s| s.bg(hover_bg))
        })
        .when(!enabled, |button| button.opacity(0.5))
        .children(icon.map(|icon| icon_sized(icon, 14., icon_color)))
        .child(
            div()
                .whitespace_nowrap()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(text)
                .child(label.into()),
        )
}

/// 主按钮（实心强调色）：页面或弹窗的主要操作
pub fn primary_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    button(id, ButtonKind::Primary, Some(icon), label, true, p)
}

/// 次级按钮（描边）：与主按钮并排的次要操作
pub fn secondary_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    button(id, ButtonKind::Secondary, Some(icon), label, true, p)
}

/// 只有文字的按钮（弹窗底栏的取消、保存等）
pub fn text_button(
    id: impl Into<ElementId>,
    kind: ButtonKind,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    button(id, kind, None, label, true, p)
}

/// 图标按钮：悬停时显示底色、图标提亮，并弹出提示文字
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    tooltip: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    icon_button_colored(id, icon, p.text_muted, p.text, tooltip, p)
}

/// 同 [`icon_button`]，但自行指定图标的常态色和悬停色（例如复制成功后的绿色对勾）
pub fn icon_button_colored(
    id: impl Into<ElementId>,
    icon: &'static str,
    icon_color: Hsla,
    hover_icon_color: Hsla,
    tooltip: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    let hover_bg = p.hover_strong;
    div()
        .id(id)
        .group(ICON_BUTTON_GROUP)
        .flex_none()
        .size(ICON_BUTTON_SIZE)
        .rounded(px(6.))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(hover_bg))
        .tooltip(text_tooltip(tooltip))
        .child(
            icon_sized(icon, 14., icon_color)
                .group_hover(ICON_BUTTON_GROUP, move |s| s.text_color(hover_icon_color)),
        )
}

/// 生成纯文字提示，用于 `.tooltip(...)`
pub fn text_tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| Tooltip::new(text.clone()).build(window, cx)
}
