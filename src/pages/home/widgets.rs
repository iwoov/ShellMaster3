// 主页各页面共用的界面部件

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::tooltip::Tooltip;
use gpui_component::ActiveTheme;

use crate::components::common::icon::icon_sized;
use crate::theme::Palette;

/// 侧边栏宽度（会话视图左上角的品牌区与之等宽）
pub const SIDEBAR_WIDTH: Pixels = px(220.);
/// 窗口顶部条的高度。主页的品牌区、标签条和会话视图的标题栏共用这个高度，
/// 标签页在两个视图之间切换时位置不会跳动。
pub const TOPBAR_HEIGHT: Pixels = px(48.);
/// 页面内容的左右留白
pub const CONTENT_PADDING: Pixels = px(24.);
/// 图标按钮的边长
pub const ICON_BUTTON_SIZE: Pixels = px(28.);

/// 图标按钮内部用于联动悬停样式的分组名
const ICON_BUTTON_GROUP: &str = "icon-button";

/// 页面标题行：左侧是标题和计数，调用方用 `.child()` 在右侧追加该页的操作控件
pub fn page_header(
    title: impl Into<SharedString>,
    count: Option<SharedString>,
    p: &Palette,
) -> Div {
    div()
        .flex_none()
        .h(px(32.))
        .mt(px(2.))
        .mb(px(14.))
        .px(CONTENT_PADDING)
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_none()
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title.into()),
        )
        .children(count.map(|count| {
            div()
                .flex_none()
                .pt(px(3.))
                .text_size(px(13.))
                .text_color(p.text_faint)
                .child(count)
        }))
        .child(div().flex_1())
}

/// 主按钮（实心强调色）：页面的主要操作
pub fn primary_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    let hover_bg = p.primary_hover;
    div()
        .id(id)
        .flex_none()
        .h(px(30.))
        .pl(px(9.))
        .pr(px(12.))
        .rounded(px(7.))
        .bg(p.primary)
        .hover(move |s| s.bg(hover_bg))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(5.))
        .child(icon_sized(icon, 14., p.primary_foreground))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.primary_foreground)
                .child(label.into()),
        )
}

/// 次级按钮（描边）：与主按钮并排的次要操作
pub fn secondary_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Stateful<Div> {
    let hover_bg = p.hover_strong;
    div()
        .id(id)
        .flex_none()
        .h(px(30.))
        .pl(px(9.))
        .pr(px(12.))
        .rounded(px(7.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .hover(move |s| s.bg(hover_bg))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(5.))
        .child(icon_sized(icon, 14., p.text_muted))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.text)
                .child(label.into()),
        )
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

/// 按最小列宽计算一行能排下的列数（至少 1 列），用于让卡片铺满整行
pub fn grid_columns(available: Pixels, min_column_width: Pixels, gap: Pixels) -> u16 {
    let columns = ((available + gap) / (min_column_width + gap)).floor();
    // 负数和 NaN 转成 0，再保证至少 1 列
    (columns as u16).max(1)
}

/// 把 `available` 均分成 `columns` 列后每一列的宽度
pub fn grid_cell_width(available: Pixels, columns: u16, gap: Pixels) -> Pixels {
    let columns = columns.max(1) as f32;
    ((available - gap * (columns - 1.)) / columns).max(px(0.))
}

/// 文字被截断时末尾显示的省略号
const ELLIPSIS: &str = "…";

/// 截断位置：`text` 在 `cut` 字节处（向前对齐到字符边界）截断，去掉末尾空白后接省略号
fn ellipsize(text: &str, mut cut: usize) -> String {
    cut = cut.min(text.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{}", text[..cut].trim_end(), ELLIPSIS)
}

/// 裁剪到指定宽度后的文字
pub struct Fitted {
    pub text: SharedString,
    /// 是否有内容被省略
    pub truncated: bool,
}

/// 按实际排版宽度把单行文字裁到指定宽度，放不下时末尾加省略号。
///
/// 这里不用 GPUI 的 `.truncate()`：文字元素只按第一次测量的结果排版，而 flex 行里的元素第一次
/// 测量时还拿不到实际宽度，文字最终只会被硬切，看不出是否被截断（被硬切的主机名看起来像另一个
/// 完整的地址）。调用方给出确定的可用宽度，文字在渲染前就裁好，结果与布局过程无关。
#[derive(Clone)]
pub struct TextFitter {
    text_system: Arc<WindowTextSystem>,
    ui_family: SharedString,
    mono_family: SharedString,
}

impl TextFitter {
    pub fn new(window: &Window, cx: &App) -> Self {
        Self {
            text_system: window.text_system().clone(),
            ui_family: cx.theme().font_family.clone(),
            mono_family: cx.theme().mono_font_family.clone(),
        }
    }

    /// 排版一行文字（结果由文字系统缓存，同样的文字每帧只排一次）
    fn layout(
        &self,
        family: &SharedString,
        text: &str,
        size: f32,
        weight: FontWeight,
    ) -> Arc<LineLayout> {
        let run = TextRun {
            len: text.len(),
            font: Font {
                weight,
                ..font(family.clone())
            },
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        self.text_system.layout_line(text, px(size), &[run], None)
    }

    fn fit(
        &self,
        family: &SharedString,
        text: &str,
        max_width: Pixels,
        size: f32,
        weight: FontWeight,
    ) -> Fitted {
        // 单行显示：换行符当作空格
        let text = text.replace(['\n', '\r'], " ");
        let layout = self.layout(family, &text, size, weight);
        if layout.width <= max_width {
            return Fitted {
                text: text.into(),
                truncated: false,
            };
        }

        let ellipsis_width = self.layout(family, ELLIPSIS, size, weight).width;
        // 省略号之前还能放下的宽度；取落在这个位置上的字符的起点作为截断处
        let cut = layout
            .index_for_x(max_width - ellipsis_width)
            .unwrap_or(text.len());
        Fitted {
            text: ellipsize(&text, cut).into(),
            truncated: true,
        }
    }

    /// 界面字体下一行文字的宽度
    pub fn width(&self, text: &str, size: f32, weight: FontWeight) -> Pixels {
        self.layout(&self.ui_family, text, size, weight).width
    }

    /// 用界面字体显示的单行文字，超出 `max_width` 时末尾显示省略号。
    /// 字号和字重同时用于量宽度和渲染，两者不会不一致。
    pub fn label(&self, text: &str, max_width: Pixels, size: f32, weight: FontWeight) -> Div {
        let fitted = self.fit(&self.ui_family, text, max_width, size, weight);
        div()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(size))
            .font_weight(weight)
            .child(fitted.text)
    }

    /// 同 [`Self::label`]，另外在文字被截断时，鼠标悬停会显示完整内容
    pub fn label_with_tooltip(
        &self,
        id: impl Into<ElementId>,
        text: &str,
        max_width: Pixels,
        size: f32,
        weight: FontWeight,
    ) -> Stateful<Div> {
        let fitted = self.fit(&self.ui_family, text, max_width, size, weight);
        div()
            .id(id)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(size))
            .font_weight(weight)
            .when(fitted.truncated, |label| {
                label.tooltip(text_tooltip(text.to_string()))
            })
            .child(fitted.text)
    }

    /// 等宽文字的容器：设置字体和字号，内容由调用方填入
    pub fn mono_container(&self, size: f32) -> Div {
        div()
            .overflow_hidden()
            .whitespace_nowrap()
            .font_family(self.mono_family.clone())
            .text_size(px(size))
    }

    /// 用等宽字体显示的单行文字，超出 `max_width` 时末尾显示省略号
    pub fn mono_label(&self, text: &str, max_width: Pixels, size: f32) -> Div {
        self.mono_container(size)
            .child(self.fit_mono(text, max_width, size).text)
    }

    /// 把等宽文字裁到 `max_width` 以内
    pub fn fit_mono(&self, text: &str, max_width: Pixels, size: f32) -> Fitted {
        self.fit(&self.mono_family, text, max_width, size, FontWeight::NORMAL)
    }

    /// 等宽字体下一行文字的宽度
    pub fn mono_width(&self, text: &str, size: f32) -> Pixels {
        self.layout(&self.mono_family, text, size, FontWeight::NORMAL)
            .width
    }
}

#[cfg(test)]
mod tests {
    use super::{ellipsize, grid_cell_width, grid_columns};
    use gpui::px;

    #[test]
    fn grid_columns_fills_the_row() {
        // 3 列 264 + 2 个间距 12 = 816
        assert_eq!(grid_columns(px(816.), px(264.), px(12.)), 3);
        assert_eq!(grid_columns(px(815.), px(264.), px(12.)), 2);
        assert_eq!(grid_columns(px(932.), px(264.), px(12.)), 3);
        assert_eq!(grid_columns(px(1332.), px(264.), px(12.)), 4);
    }

    #[test]
    fn grid_columns_never_returns_zero() {
        assert_eq!(grid_columns(px(100.), px(264.), px(12.)), 1);
        assert_eq!(grid_columns(px(0.), px(264.), px(12.)), 1);
        assert_eq!(grid_columns(px(-50.), px(264.), px(12.)), 1);
    }

    #[test]
    fn grid_cells_share_the_width_left_after_gaps() {
        assert_eq!(grid_cell_width(px(932.), 3, px(12.)), px(908.) / 3.);
        assert_eq!(grid_cell_width(px(500.), 1, px(12.)), px(500.));
        // 列数为 0 时按 1 列处理；放不下时宽度为 0 而不是负数
        assert_eq!(grid_cell_width(px(500.), 0, px(12.)), px(500.));
        assert_eq!(grid_cell_width(px(10.), 3, px(12.)), px(0.));
    }

    #[test]
    fn ellipsize_cuts_on_character_boundaries() {
        assert_eq!(ellipsize("db-master.prod.internal", 9), "db-master…");
        // 截断处落在多字节字符中间时退到它的起点
        assert_eq!(ellipsize("订单服务主库", 4), "订…");
        // 省略号前不留空白
        assert_eq!(ellipsize("Nginx 网关", 6), "Nginx…");
        assert_eq!(ellipsize("abc", 0), "…");
        assert_eq!(ellipsize("abc", 99), "abc…");
    }
}
