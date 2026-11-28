// 对话框组件：统一的遮罩、外框、头部、底栏和侧栏导航项，以及删除确认对话框

use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::button::{icon_button, text_button, ButtonKind};
use super::icon::icon_sized;
use crate::constants::icons;
use crate::theme::Palette;

/// 弹窗外框的圆角
pub const DIALOG_RADIUS: Pixels = px(12.);
/// 弹窗与窗口边缘之间至少保留的距离
pub const DIALOG_MARGIN: Pixels = px(24.);

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// 弹窗尺寸：不超过 `preferred`，也不超出窗口（四周各留 [`DIALOG_MARGIN`]）
pub fn fit_to_viewport(preferred: Size<Pixels>, viewport: Size<Pixels>) -> Size<Pixels> {
    size(
        preferred
            .width
            .min(viewport.width - DIALOG_MARGIN * 2.)
            .max(px(0.)),
        preferred
            .height
            .min(viewport.height - DIALOG_MARGIN * 2.)
            .max(px(0.)),
    )
}

/// 弹窗打开期间每帧调用一次（在渲染时），保证焦点留在弹窗里。
///
/// 获得焦点的输入框被移除后（切换分区、切换认证方式、关掉开关等），焦点会停在一个已经不显示的
/// 元素上，按键不再经过弹窗，Esc 和回车都会失效。这里等这一帧画完再检查：焦点不在弹窗里，
/// 就交还给遮罩跟踪的 `handle`。弹窗已关闭（`is_open` 返回 false）时什么也不做。
pub fn keep_focus_in_dialog<T: 'static>(
    state: WeakEntity<T>,
    is_open: fn(&T) -> bool,
    handle: &FocusHandle,
    window: &Window,
) {
    let handle = handle.clone();
    window.on_next_frame(move |window, cx| {
        let open = state
            .read_with(cx, |state, _| is_open(state))
            .unwrap_or(false);
        if open && !handle.contains_focused(window, cx) {
            window.focus(&handle, cx);
        }
    });
}

/// 弹窗遮罩：铺满父容器并拦截下层的悬停、点击和滚动，弹窗居中显示。
///
/// - Esc / 回车：遮罩跟踪的焦点或弹窗内获得焦点的控件收到按键后，冒泡到遮罩处理。
///   单行输入框会把回车和 Esc 继续向上传，多行输入框和下拉菜单会自己处理掉。
/// - 点遮罩：只有设置了 [`Self::on_backdrop_click`] 才响应，表单类弹窗不设置，避免误触丢失输入。
#[derive(Default)]
pub struct DialogOverlay {
    focus_handle: Option<FocusHandle>,
    on_escape: Option<Handler>,
    on_enter: Option<Handler>,
    on_backdrop: Option<Handler>,
}

impl DialogOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    /// 遮罩跟踪的焦点：弹窗内没有输入框获得焦点时，按键由它接收
    pub fn focus(mut self, handle: Option<&FocusHandle>) -> Self {
        self.focus_handle = handle.cloned();
        self
    }

    pub fn on_escape(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_escape = Some(Rc::new(handler));
        self
    }

    pub fn on_enter(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_enter = Some(Rc::new(handler));
        self
    }

    pub fn on_backdrop_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_backdrop = Some(Rc::new(handler));
        self
    }

    pub fn render(
        self,
        id: impl Into<ElementId>,
        panel: impl IntoElement,
        p: &Palette,
    ) -> Stateful<Div> {
        let DialogOverlay {
            focus_handle,
            on_escape,
            on_enter,
            on_backdrop,
        } = self;

        div()
            .id(id)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(p.overlay)
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .when_some(focus_handle, |overlay, handle| overlay.track_focus(&handle))
            // 弹窗自身会拦下按下事件，能到这里的只有点在遮罩上的
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                if let Some(handler) = &on_backdrop {
                    handler(window, cx);
                }
            })
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .on_key_down(move |event, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.modifiers.modified() {
                    return;
                }
                let handler = match keystroke.key.as_str() {
                    "escape" => on_escape.as_ref(),
                    "enter" => on_enter.as_ref(),
                    _ => None,
                };
                if let Some(handler) = handler {
                    cx.stop_propagation();
                    handler(window, cx);
                }
            })
            .child(panel)
    }
}

/// 弹窗外框。点击和滚动不会穿透到遮罩。
pub fn dialog_panel(p: &Palette) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .rounded(DIALOG_RADIUS)
        .shadow_xl()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_scroll(|_, _, cx| cx.stop_propagation())
}

/// 弹窗头部：标题、可选的说明，右上角是关闭按钮
pub fn dialog_header(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    close_tooltip: impl Into<SharedString>,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    p: &Palette,
) -> Div {
    div()
        .flex_none()
        .flex()
        .items_start()
        .gap(px(12.))
        .pl(px(20.))
        .pr(px(12.))
        .pt(px(16.))
        .pb(px(4.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pt(px(2.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(title.into()),
                )
                .children(description.map(|description| {
                    div()
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(description)
                })),
        )
        .child(
            icon_button("dialog-close-btn", icons::X, close_tooltip, p)
                .on_click(move |_, window, cx| on_close(window, cx)),
        )
}

/// 弹窗底栏：左边是提示（错误）文字，调用方用 `.child()` 在右边追加按钮
pub fn dialog_footer(message: Option<SharedString>, p: &Palette) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(20.))
        .pt(px(16.))
        .pb(px(18.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(p.danger)
                .children(message),
        )
}

/// 双栏弹窗左侧导航的一项：30px 高，选中时有底色、图标变为主色；`marked` 时右侧显示一个小圆点
pub fn dialog_nav_item(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: impl Into<SharedString>,
    selected: bool,
    marked: bool,
    p: &Palette,
) -> Stateful<Div> {
    let (hover_bg, hover_text) = (p.hover, p.text);
    div()
        .id(id)
        .flex_none()
        .h(px(30.))
        .px(px(8.))
        .rounded(px(6.))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(9.))
        .text_size(px(13.))
        .text_color(p.text_muted)
        .when(selected, |item| {
            item.bg(p.selected)
                .text_color(p.text)
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |item| {
            item.hover(move |s| s.bg(hover_bg).text_color(hover_text))
        })
        .child(icon_sized(
            icon,
            16.,
            if selected { p.primary } else { p.text_muted },
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .child(label.into()),
        )
        .when(marked, |item| {
            item.child(div().flex_none().size(px(6.)).rounded_full().bg(p.primary))
        })
}

/// 双栏弹窗的左栏：侧栏底色，顶部是弹窗标题。调用方追加 [`dialog_nav_list`]。
pub fn dialog_sidebar(title: impl Into<SharedString>, width: Pixels, p: &Palette) -> Div {
    // 弹窗外框的裁剪是直角的，左栏底色需要自己切出左侧圆角
    let corner = DIALOG_RADIUS - px(1.);
    div()
        .w(width)
        .h_full()
        .flex_none()
        .bg(p.sidebar)
        .border_r_1()
        .border_color(p.border)
        .rounded_tl(corner)
        .rounded_bl(corner)
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .h(px(56.))
                .px(px(18.))
                .flex()
                .items_center()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title.into()),
        )
}

/// 左栏里的导航列表（项目多时可滚动），子项用 [`dialog_nav_item`]
pub fn dialog_nav_list(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .px(px(8.))
        .pb(px(8.))
        .flex()
        .flex_col()
        .gap(px(2.))
}

/// 双栏弹窗右侧的标题栏：当前分区的标题，右边是关闭按钮
pub fn dialog_pane_header(
    title: impl Into<SharedString>,
    close_tooltip: impl Into<SharedString>,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    padding_x: Pixels,
    p: &Palette,
) -> Div {
    div()
        .flex_none()
        .h(px(56.))
        .pl(padding_x)
        .pr(px(14.))
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title.into()),
        )
        .child(
            icon_button("dialog-close-btn", icons::X, close_tooltip, p)
                .on_click(move |_, window, cx| on_close(window, cx)),
        )
}

/// 双栏弹窗右侧的滚动内容区。内边距放在滚动区里面，滚到底时不会被裁掉。
pub fn dialog_scroll_area(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    padding_x: Pixels,
    content: impl IntoElement,
) -> Div {
    div()
        .flex_1()
        .min_h(px(0.))
        .relative()
        .child(
            div()
                .id(id)
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .child(div().px(padding_x).pt(px(4.)).pb(px(24.)).child(content)),
        )
        .vertical_scrollbar(scroll_handle)
}

/// 双栏弹窗右侧的底栏：上边一条分隔线，左边是错误提示，调用方用 `.child()` 在右边追加按钮
pub fn dialog_pane_footer(message: Option<SharedString>, padding_x: Pixels, p: &Palette) -> Div {
    div()
        .flex_none()
        .h(px(60.))
        .px(padding_x)
        .border_t_1()
        .border_color(p.line)
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(p.danger)
                .children(message),
        )
}

/// 确认对话框的文案
pub struct ConfirmDialogText {
    pub title: SharedString,
    pub message: SharedString,
    pub confirm: SharedString,
    pub cancel: SharedString,
}

/// 渲染一个用于危险操作（删除等）的确认对话框覆盖层。
///
/// `focus_handle` 由调用方在打开对话框时创建并聚焦，对话框据此响应 Esc 取消。
/// 确认只能点击按钮：危险操作不设回车默认键，避免连按回车误删。
pub fn render_confirm_dialog(
    text: ConfirmDialogText,
    focus_handle: &FocusHandle,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let on_cancel_for_close = on_cancel.clone();
    let on_cancel_for_button = on_cancel.clone();

    let panel = dialog_panel(&p)
        .w(px(400.))
        .child(dialog_header(
            text.title,
            Some(text.message),
            text.cancel.clone(),
            on_cancel_for_close,
            &p,
        ))
        .child(
            dialog_footer(None, &p)
                .child(
                    text_button(
                        "confirm-dialog-cancel-btn",
                        ButtonKind::Secondary,
                        text.cancel,
                        &p,
                    )
                    .on_click(move |_, window, cx| on_cancel_for_button(window, cx)),
                )
                .child(
                    text_button(
                        "confirm-dialog-confirm-btn",
                        ButtonKind::Danger,
                        text.confirm,
                        &p,
                    )
                    .on_click(move |_, window, cx| on_confirm(window, cx)),
                ),
        );

    DialogOverlay::new()
        .focus(Some(focus_handle))
        .on_escape(on_cancel)
        .render("confirm-dialog-overlay", panel, &p)
}

#[cfg(test)]
mod tests {
    use super::fit_to_viewport;
    use gpui_kit::{px, size};

    #[test]
    fn dialogs_shrink_to_fit_the_window() {
        // 窗口足够大时用期望尺寸
        assert_eq!(
            fit_to_viewport(size(px(860.), px(600.)), size(px(1200.), px(800.))),
            size(px(860.), px(600.))
        );
        // 窗口较小时四周各留 24px
        assert_eq!(
            fit_to_viewport(size(px(860.), px(600.)), size(px(860.), px(600.))),
            size(px(812.), px(552.))
        );
        // 不会出现负数
        assert_eq!(
            fit_to_viewport(size(px(860.), px(600.)), size(px(20.), px(20.))),
            size(px(0.), px(0.))
        );
    }
}
