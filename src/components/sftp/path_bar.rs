// SFTP 路径栏组件
// 显示模式：可点击的面包屑（放不下时从中间折叠成「…」，保证当前目录可见）；
// 编辑模式：与显示模式同样大小的输入框，回车跳转，非法路径标红

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::input::{InputEvent, InputState};

use crate::components::common::button::{text_tooltip, CONTROL_RADIUS};
use crate::components::common::icon::icon_sized;
use crate::components::common::input::{field_error, text_field};
use crate::components::common::panel::{spinner, HEADER_CONTROL_HEIGHT};
use crate::components::common::text::TextFitter;
use crate::constants::icons;

/// 面包屑每段文字的字号和左右留白
const SEGMENT_TEXT_SIZE: f32 = 12.;
const SEGMENT_PADDING: f32 = 5.;
/// 两段之间的分隔箭头（含左右间距）宽度
const SEPARATOR_WIDTH: f32 = 14.;
/// 路径框左右内边距、右侧加载图标和最小的可点击空白
const BAR_PADDING: f32 = 6.;
const SPINNER_SLOT: f32 = 18.;
const MIN_EDIT_AREA: f32 = 16.;

/// 路径栏事件
#[derive(Clone, Debug)]
pub enum PathBarEvent {
    /// 导航到指定路径
    Navigate(String),
}

/// 路径栏状态
pub struct PathBarState {
    /// 是否处于编辑模式
    is_editing: bool,
    /// 输入框状态
    input_state: Entity<InputState>,
    /// 当前路径
    current_path: String,
    /// 列表正在加载当前目录（路径框右侧显示加载图标）
    loading: bool,
    /// 输入的路径不合法（不是绝对路径），输入框标红
    invalid: bool,
    /// 上一帧路径框的宽度，用来决定面包屑要不要折叠
    bar_width: Pixels,
    /// 事件回调
    on_event: Rc<dyn Fn(PathBarEvent, &mut App)>,
}

impl PathBarState {
    pub fn new<F>(window: &mut Window, cx: &mut Context<Self>, on_event: F) -> Self
    where
        F: Fn(PathBarEvent, &mut App) + 'static,
    {
        let input_state = cx.new(|cx| InputState::new(window, cx));

        cx.subscribe(
            &input_state,
            |this, _input, event: &InputEvent, cx| match event {
                // 按下回车，导航到新路径
                InputEvent::PressEnter { .. } => this.confirm_edit(cx),
                // 失焦时取消编辑
                InputEvent::Blur => this.cancel_edit(cx),
                // 重新输入时去掉错误标记
                InputEvent::Change => {
                    if this.invalid {
                        this.invalid = false;
                        cx.notify();
                    }
                }
                _ => {}
            },
        )
        .detach();

        Self {
            is_editing: false,
            input_state,
            current_path: String::new(),
            loading: false,
            invalid: false,
            bar_width: px(0.),
            on_event: Rc::new(on_event),
        }
    }

    /// 更新当前路径（每帧都会调用，只有变化时才重绘）
    pub fn set_path(&mut self, path: &str, _window: &mut Window, cx: &mut Context<Self>) {
        if self.current_path != path {
            self.current_path = path.to_string();
            cx.notify();
        }
    }

    /// 更新加载状态
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        if self.loading != loading {
            self.loading = loading;
            cx.notify();
        }
    }

    /// 进入编辑模式
    pub fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_editing = true;
        self.invalid = false;

        let path = self.current_path.clone();
        self.input_state.update(cx, |input, cx| {
            input.set_value(&path, window, cx);
            input.focus(window, cx);
        });

        cx.notify();
    }

    /// 确认编辑：绝对路径才跳转，否则标红并留在编辑状态
    fn confirm_edit(&mut self, cx: &mut Context<Self>) {
        if !self.is_editing {
            return;
        }

        let new_path = self.input_state.read(cx).value().trim().to_string();
        if !is_absolute_path(&new_path) {
            self.invalid = true;
            cx.notify();
            return;
        }

        self.is_editing = false;
        self.invalid = false;
        let on_event = self.on_event.clone();
        on_event(PathBarEvent::Navigate(new_path), cx);
        cx.notify();
    }

    /// 取消编辑
    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if !self.is_editing {
            return;
        }
        self.is_editing = false;
        self.invalid = false;
        cx.notify();
    }

    /// 导航到指定路径（面包屑点击）
    fn navigate_to(&mut self, path: String, cx: &mut Context<Self>) {
        let on_event = self.on_event.clone();
        on_event(PathBarEvent::Navigate(path), cx);
    }
}

impl Render for PathBarState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = crate::theme::palette(cx);

        if self.is_editing {
            // 编辑模式：与显示模式同样高度和圆角的输入框
            let field = field_error(text_field(&self.input_state), self.invalid, &p);
            let invalid_hint = crate::i18n::t(
                &crate::services::storage::cached_settings().theme.language,
                "sftp.path.invalid",
            );
            return div()
                .id("sftp-path-bar-input")
                .flex_1()
                .min_w_0()
                .when(self.invalid, |bar| bar.tooltip(text_tooltip(invalid_hint)))
                .child(gpui_kit::Styled::h(field, HEADER_CONTROL_HEIGHT))
                .into_any_element();
        }

        let fit = TextFitter::new(window, cx);
        let segments = parse_path_segments(&self.current_path);
        let widths: Vec<f32> = segments
            .iter()
            .map(|(name, _)| {
                f32::from(fit.width(name, SEGMENT_TEXT_SIZE, FontWeight::MEDIUM))
                    + SEGMENT_PADDING * 2.
            })
            .collect();
        let ellipsis_width =
            f32::from(fit.width("…", SEGMENT_TEXT_SIZE, FontWeight::NORMAL)) + SEGMENT_PADDING * 2.;
        let available =
            (f32::from(self.bar_width) - BAR_PADDING * 2. - SPINNER_SLOT - MIN_EDIT_AREA).max(0.);
        let tail = collapse_tail(&widths, available, SEPARATOR_WIDTH, ellipsis_width);

        // 要显示的段：根目录，折叠时接「…」，再接最后若干段
        let count = segments.len();
        let visible: Vec<usize> = match tail {
            Some(keep) => std::iter::once(0).chain(count - keep..count).collect(),
            None => (0..count).collect(),
        };
        // 当前目录（最后一段）可用的宽度：总宽度减去前面各段，至少留 40
        let others: f32 = visible[..visible.len().saturating_sub(1)]
            .iter()
            .map(|&ix| widths[ix] + SEPARATOR_WIDTH)
            .sum::<f32>()
            + if tail.is_some() {
                ellipsis_width + SEPARATOR_WIDTH
            } else {
                0.
            };
        let last_width = px((available - others - SEGMENT_PADDING * 2.).max(40.));

        let entity = cx.entity();
        let mut bar = div()
            .id("sftp-path-bar")
            .relative()
            .flex_1()
            .min_w_0()
            .h(HEADER_CONTROL_HEIGHT)
            .px(px(BAR_PADDING))
            .rounded(CONTROL_RADIUS)
            .bg(p.field)
            .border_1()
            .border_color(p.field_border)
            .flex()
            .items_center()
            .overflow_hidden()
            // 记下路径框的宽度，下一帧据此决定是否折叠
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let width = bounds.size.width;
                        entity.update(cx, |this, cx| {
                            if (this.bar_width - width).abs() > px(1.) {
                                this.bar_width = width;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );

        for (position, &ix) in visible.iter().enumerate() {
            if position > 0 {
                bar = bar.child(
                    div()
                        .flex_none()
                        .w(px(SEPARATOR_WIDTH))
                        .flex()
                        .justify_center()
                        .child(icon_sized(icons::CHEVRON_RIGHT, 10., p.text_faint)),
                );
            }
            // 折叠的中间部分
            if position == 1 && tail.is_some() {
                let hidden_path = segments[count - tail.unwrap_or(1) - 1].1.clone();
                bar = bar
                    .child(
                        div()
                            .id("sftp-crumb-ellipsis")
                            .flex_none()
                            .h(px(22.))
                            .px(px(SEGMENT_PADDING))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .text_size(px(SEGMENT_TEXT_SIZE))
                            .text_color(p.text_faint)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover).text_color(p.text))
                            .tooltip(text_tooltip(hidden_path.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.navigate_to(hidden_path.clone(), cx);
                            }))
                            .child("…"),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(SEPARATOR_WIDTH))
                            .flex()
                            .justify_center()
                            .child(icon_sized(icons::CHEVRON_RIGHT, 10., p.text_faint)),
                    );
            }

            let (name, path) = segments[ix].clone();
            let is_last = ix == count - 1;
            let segment = div()
                .id(SharedString::from(format!("sftp-crumb-{ix}")))
                .flex_none()
                .h(px(22.))
                .px(px(SEGMENT_PADDING))
                .rounded(px(5.))
                .flex()
                .items_center()
                .cursor_pointer();
            bar = bar.child(if is_last {
                // 当前目录：高亮，点击进入编辑
                segment
                    .text_color(p.text)
                    .hover(move |s| s.bg(p.hover))
                    .on_click(cx.listener(|this, _, window, cx| this.start_edit(window, cx)))
                    .child(fit.label(&name, last_width, SEGMENT_TEXT_SIZE, FontWeight::MEDIUM))
            } else {
                segment
                    .text_size(px(SEGMENT_TEXT_SIZE))
                    .whitespace_nowrap()
                    .text_color(p.text_muted)
                    .hover(move |s| s.bg(p.hover).text_color(p.text))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate_to(path.clone(), cx);
                    }))
                    .child(name)
            });
        }

        bar.child(
            // 右侧空白：点击进入编辑模式
            div()
                .id("sftp-path-bar-edit-trigger")
                .flex_1()
                .min_w(px(MIN_EDIT_AREA))
                .h_full()
                .cursor_text()
                .on_click(cx.listener(|this, _, window, cx| this.start_edit(window, cx))),
        )
        .when(self.loading, |bar| {
            bar.child(
                div()
                    .flex_none()
                    .w(px(SPINNER_SLOT))
                    .flex()
                    .justify_end()
                    .child(spinner("sftp-path-loading", 12., p.text_faint)),
            )
        })
        .into_any_element()
    }
}

/// 是否为绝对路径（只接受以 / 开头的路径）
fn is_absolute_path(path: &str) -> bool {
    path.starts_with('/')
}

/// 解析路径为面包屑段
/// 例如："/home/alice" -> [("/", "/"), ("home", "/home"), ("alice", "/home/alice")]
fn parse_path_segments(path: &str) -> Vec<(String, String)> {
    let mut segments = Vec::new();

    if path.is_empty() || !path.starts_with('/') {
        return segments;
    }

    // 根目录
    segments.push(("/".to_string(), "/".to_string()));

    // 其他路径段
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current_path = String::new();

    for part in parts {
        current_path.push('/');
        current_path.push_str(part);
        segments.push((part.to_string(), current_path.clone()));
    }

    segments
}

/// 面包屑各段（含留白）放不下时从中间折叠：保留根目录和最后 k 段，中间换成「…」。
/// 全部放得下（或没有可折叠的中间段）时返回 `None`；否则返回 k，至少为 1。
fn collapse_tail(widths: &[f32], available: f32, separator: f32, ellipsis: f32) -> Option<usize> {
    let count = widths.len();
    let total = widths.iter().sum::<f32>() + separator * count.saturating_sub(1) as f32;
    if count <= 2 || total <= available {
        return None;
    }
    let head = widths[0] + separator + ellipsis + separator;
    // 中间至少折叠一段，所以最多保留 count - 2 段
    for keep in (1..=count - 2).rev() {
        let tail = widths[count - keep..].iter().sum::<f32>() + separator * (keep - 1) as f32;
        if head + tail <= available {
            return Some(keep);
        }
    }
    Some(1)
}

#[cfg(test)]
mod tests {
    use super::{collapse_tail, is_absolute_path, parse_path_segments};

    #[test]
    fn segments_start_at_the_root() {
        assert_eq!(
            parse_path_segments("/var/www"),
            vec![
                ("/".to_string(), "/".to_string()),
                ("var".to_string(), "/var".to_string()),
                ("www".to_string(), "/var/www".to_string()),
            ]
        );
        assert_eq!(parse_path_segments("/").len(), 1);
        assert!(parse_path_segments("relative/path").is_empty());
    }

    #[test]
    fn crumbs_that_fit_are_not_collapsed() {
        // 4 段各 20、3 个分隔各 10：共 110
        assert_eq!(collapse_tail(&[20., 20., 20., 20.], 110., 10., 12.), None);
        // 只有根目录和一段时没有可折叠的中间段
        assert_eq!(collapse_tail(&[20., 300.], 100., 10., 12.), None);
    }

    #[test]
    fn crumbs_collapse_from_the_middle_keeping_the_longest_tail() {
        let widths = [20., 40., 40., 40., 40.];
        // 根目录 20 + 分隔 10 + 省略号 12 + 分隔 10 = 52；保留 2 段：40 + 10 + 40 = 90 → 142
        assert_eq!(collapse_tail(&widths, 142., 10., 12.), Some(2));
        assert_eq!(collapse_tail(&widths, 141., 10., 12.), Some(1));
        // 再窄也至少保留当前目录
        assert_eq!(collapse_tail(&widths, 10., 10., 12.), Some(1));
        // 最多保留 count - 2 段（中间至少折叠一段）：全部 220 放不下，保留 3 段需要 52 + 140 = 192
        assert_eq!(collapse_tail(&widths, 200., 10., 12.), Some(3));
    }

    #[test]
    fn only_absolute_paths_are_accepted() {
        assert!(is_absolute_path("/etc/nginx"));
        assert!(!is_absolute_path("etc/nginx"));
        assert!(!is_absolute_path(""));
    }
}
