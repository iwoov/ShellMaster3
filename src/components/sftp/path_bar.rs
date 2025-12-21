// SFTP 路径栏组件
// 支持面包屑导航模式和输入编辑模式

use std::rc::Rc;

use gpui::*;
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme, Sizable};

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
    /// 事件回调
    on_event: Rc<dyn Fn(PathBarEvent, &mut App)>,
}

impl PathBarState {
    pub fn new<F>(window: &mut Window, cx: &mut Context<Self>, on_event: F) -> Self
    where
        F: Fn(PathBarEvent, &mut App) + 'static,
    {
        let input_state = cx.new(|cx| InputState::new(window, cx));

        // 监听输入框的失焦事件
        cx.subscribe(&input_state, |this, _input, event: &InputEvent, cx| {
            match event {
                InputEvent::PressEnter { .. } => {
                    // 按下回车，导航到新路径
                    this.confirm_edit(cx);
                }
                InputEvent::Blur => {
                    // 失焦时取消编辑
                    this.cancel_edit(cx);
                }
                _ => {}
            }
        })
        .detach();

        Self {
            is_editing: false,
            input_state,
            current_path: String::new(),
            on_event: Rc::new(on_event),
        }
    }

    /// 更新当前路径
    pub fn set_path(&mut self, path: &str, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_path = path.to_string();
        cx.notify();
    }

    /// 进入编辑模式
    pub fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_editing = true;

        let path = self.current_path.clone();

        // 设置输入框的值为当前路径，并聚焦
        self.input_state.update(cx, |input, cx| {
            input.set_value(&path, window, cx);
            // 聚焦输入框
            input.focus(window, cx);
        });

        cx.notify();
    }

    /// 确认编辑，导航到新路径
    fn confirm_edit(&mut self, cx: &mut Context<Self>) {
        if !self.is_editing {
            return;
        }

        let new_path = self.input_state.read(cx).value().to_string();
        self.is_editing = false;

        // 如果路径有效则导航
        if !new_path.is_empty() && new_path.starts_with('/') {
            let on_event = self.on_event.clone();
            on_event(PathBarEvent::Navigate(new_path), cx);
        }

        cx.notify();
    }

    /// 取消编辑
    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if !self.is_editing {
            return;
        }
        self.is_editing = false;
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
        let input_bg = cx.theme().background;
        let border_color = cx.theme().border;

        if self.is_editing {
            // 编辑模式：显示输入框
            div()
                .id("sftp-path-bar-input")
                .flex_1()
                .h(px(22.))
                .child(Input::new(&self.input_state).w_full().xsmall())
        } else {
            // 显示模式：显示面包屑
            let current_path = self.current_path.clone();
            let segments = parse_path_segments(&current_path);

            let mut breadcrumb = Breadcrumb::new().text_xs();

            for (i, (name, full_path)) in segments.iter().enumerate() {
                let path = full_path.clone();
                let is_last = i == segments.len() - 1;

                let item = BreadcrumbItem::new(name.clone()).on_click({
                    let path = path.clone();
                    cx.listener(move |this, _event, _window, cx| {
                        this.navigate_to(path.clone(), cx);
                    })
                });

                // 最后一个元素设置为当前不可点击的样式
                let item = if is_last { item.disabled(true) } else { item };

                breadcrumb = breadcrumb.child(item);
            }

            div()
                .id("sftp-path-bar")
                .flex_1()
                .h(px(22.))
                .px_2()
                .bg(input_bg)
                .border_1()
                .border_color(border_color)
                .rounded(px(4.))
                .flex()
                .items_center()
                .overflow_hidden()
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.start_edit(window, cx);
                }))
                .child(breadcrumb)
        }
    }
}

/// 解析路径为面包屑段
/// 例如："/home/wuyun" -> [("/", "/"), ("home", "/home"), ("wuyun", "/home/wuyun")]
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
