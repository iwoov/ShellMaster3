// 文字排版工具：按实际字宽把单行文字裁到指定宽度

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::button::text_tooltip;

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
    text_system: Rc<WindowTextSystem>,
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

    /// 同 [`Self::mono_label`]，另外在文字被截断时，鼠标悬停会显示完整内容
    pub fn mono_label_with_tooltip(
        &self,
        id: impl Into<ElementId>,
        text: &str,
        max_width: Pixels,
        size: f32,
    ) -> Stateful<Div> {
        let fitted = self.fit_mono(text, max_width, size);
        self.mono_container(size)
            .id(id)
            .when(fitted.truncated, |label| {
                label.tooltip(text_tooltip(text.to_string()))
            })
            .child(fitted.text)
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
    use super::ellipsize;

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
