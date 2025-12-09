use gpui::prelude::*;
use gpui::*;
use gpui_component::input::InputState;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::models::server::AuthType;

/// 左侧导航菜单类型
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DialogSection {
    #[default]
    BasicInfo,
    JumpHost,
    ProxySettings,
    OtherSettings,
}

/// 服务器弹窗状态
pub struct ServerDialogState {
    pub visible: bool,
    pub is_edit: bool,
    pub current_section: DialogSection,
    // 表单 InputState 实体（延迟创建）
    pub label_input: Option<Entity<InputState>>,
    pub host_input: Option<Entity<InputState>>,
    pub port_input: Option<Entity<InputState>>,
    pub username_input: Option<Entity<InputState>>,
    pub password_input: Option<Entity<InputState>>,
    // 认证数据
    pub auth_type: AuthType,
    pub private_key_input: Option<Entity<InputState>>,
    pub passphrase_input: Option<Entity<InputState>>,
}

impl Default for ServerDialogState {
    fn default() -> Self {
        Self {
            visible: false,
            is_edit: false,
            current_section: DialogSection::BasicInfo,
            label_input: None,
            host_input: None,
            port_input: None,
            username_input: None,
            password_input: None,
            auth_type: AuthType::Password,
            private_key_input: None,
            passphrase_input: None,
        }
    }
}

impl ServerDialogState {
    /// 确保输入框已创建（在有 window 上下文时调用）
    pub fn ensure_inputs_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.label_input.is_none() {
            self.label_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("请输入服务器名称")));
        }
        if self.host_input.is_none() {
            self.host_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("IP 或域名")));
        }
        if self.port_input.is_none() {
            self.port_input = Some(cx.new(|cx| {
                let state = InputState::new(window, cx).placeholder("22");
                state
            }));
        }
        if self.username_input.is_none() {
            self.username_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("用户名")));
        }
        if self.password_input.is_none() {
            self.password_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("密码").masked(true)));
        }
        if self.private_key_input.is_none() {
            self.private_key_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("点击选择私钥文件...")));
        }
        if self.passphrase_input.is_none() {
            self.passphrase_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("私钥密码（可选）")
                    .masked(true)
            }));
        }
    }

    pub fn open_add(&mut self) {
        self.visible = true;
        self.is_edit = false;
        self.current_section = DialogSection::BasicInfo;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }
}

/// 渲染服务器弹窗覆盖层
pub fn render_server_dialog_overlay(
    state: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let state_for_content = state.clone();

    // 使用容器包裹遮罩和弹窗，它们是兄弟元素而非父子
    div()
        .id("server-dialog-container")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        // 背景遮罩层（点击关闭）
        .child(
            div()
                .id("server-dialog-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000080))
                .on_click(move |_, _, cx| {
                    state_for_close.update(cx, |s, _| s.close());
                }),
        )
        // 弹窗内容层（独立元素，不受遮罩点击影响）
        .child(render_dialog_content(state_for_content, cx))
}

/// 渲染弹窗内容
fn render_dialog_content(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    let state_for_section = state.clone();
    let state_for_cancel = state.clone();

    div()
        .id("server-dialog-content")
        .w(px(700.))
        .h(px(500.))
        .bg(rgb(0xffffff))
        .rounded_lg()
        .shadow_lg()
        .flex()
        .overflow_hidden()
        // 阻止鼠标事件传播到底层的遮罩
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_left_menu(state_for_section))
        .child(render_right_content(state, state_for_cancel, cx))
}

/// 渲染左侧导航菜单
fn render_left_menu(state: Entity<ServerDialogState>) -> impl IntoElement {
    let sections = [
        (DialogSection::BasicInfo, "基本信息", icons::SERVER),
        (DialogSection::JumpHost, "跳板机", icons::LINK),
        (DialogSection::ProxySettings, "代理设置", icons::GLOBE),
        (DialogSection::OtherSettings, "其他设置", icons::SETTINGS),
    ];

    div()
        .w(px(180.))
        .h_full()
        .bg(rgb(0xf8fafc))
        .rounded_l_lg() // Ensure left side is rounded
        .border_r_1()
        .border_color(rgb(0xe2e8f0))
        .flex()
        .flex_col()
        .p_4()
        .gap_2()
        .children(sections.into_iter().map(|(section, label, icon)| {
            let state = state.clone();
            render_menu_item(state, section, label, icon)
        }))
}

/// 渲染单个菜单项
fn render_menu_item(
    state: Entity<ServerDialogState>,
    section: DialogSection,
    label: &'static str,
    icon: &'static str,
) -> impl IntoElement {
    let state_for_click = state.clone();

    div()
        .id(SharedString::from(format!("menu-{:?}", section)))
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .hover(|s| s.bg(rgb(0xe2e8f0)))
        .on_click(move |_, _, cx| {
            state_for_click.update(cx, |s, _| {
                s.current_section = section;
            });
        })
        .child(render_icon(icon, rgb(0x64748b).into()))
        .child(div().text_sm().text_color(rgb(0x475569)).child(label))
}

/// 渲染右侧内容区域
fn render_right_content(
    state: Entity<ServerDialogState>,
    state_for_cancel: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        // 标题栏
        .child(
            div()
                .h(px(56.))
                .flex_shrink_0()
                .border_b_1()
                .border_color(rgb(0xe2e8f0))
                .flex()
                .items_center()
                .px_6()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1e293b))
                        .child("添加服务器"),
                ),
        )
        // 表单区域
        .child(
            div()
                .id("form-scroll")
                .flex_1()
                .overflow_scroll()
                .p_4()
                .child(render_basic_info_form(state, cx)),
        )
        // 底部按钮
        .child(render_footer_buttons(state_for_cancel))
}

/// 渲染基本信息表单
fn render_basic_info_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    let state_read = state.read(cx);
    let auth_type = state_read.auth_type.clone();

    // 预先准备输入框元素
    let label_input = if let Some(input) = &state_read.label_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let host_input = if let Some(input) = &state_read.host_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let port_input = if let Some(input) = &state_read.port_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let username_input = if let Some(input) = &state_read.username_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let password_input = if let Some(input) = &state_read.password_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let private_key_input = if let Some(input) = &state_read.private_key_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let passphrase_input = if let Some(input) = &state_read.passphrase_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        // 服务器标签
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label("服务器标签", icons::SERVER))
                .child(label_input),
        )
        // 主机地址
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label("主机地址", icons::GLOBE))
                .child(host_input),
        )
        // 端口
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label("端口", icons::LINK))
                .child(port_input),
        )
        // 用户名
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label("用户名", icons::USER))
                .child(username_input),
        )
        // 认证方式切换
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label("认证方式", icons::LOCK))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .p_1()
                        .bg(rgb(0xf1f5f9))
                        .rounded_md()
                        .child(render_auth_type_button(
                            state.clone(),
                            AuthType::Password,
                            "密码",
                            auth_type == AuthType::Password,
                        ))
                        .child(render_auth_type_button(
                            state.clone(),
                            AuthType::PublicKey,
                            "公钥",
                            auth_type == AuthType::PublicKey,
                        )),
                ),
        )
        // 动态渲染认证字段
        .children(match auth_type {
            AuthType::Password => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(render_form_label("密码", icons::LOCK))
                    .child(password_input)
                    .into_any_element(),
            ),
            AuthType::PublicKey => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label("私钥路径", icons::CODE))
                            .child(private_key_input),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label("私钥密码 (可选)", icons::LOCK))
                            .child(passphrase_input),
                    )
                    .into_any_element(),
            ),
        })
}

/// 渲染认证方式切换按钮
fn render_auth_type_button(
    state: Entity<ServerDialogState>,
    auth_type: AuthType,
    label: &'static str,
    selected: bool,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(if selected {
            rgb(0xffffff)
        } else {
            rgb(0xf1f5f9)
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            state.update(cx, |s, _| {
                s.auth_type = auth_type.clone();
            });
        })
        .shadow(if selected {
            vec![BoxShadow {
                // Corrected macro usage
                color: rgba(0x00000010).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            }]
        } else {
            vec![] // Corrected macro usage
        })
        .child(
            div()
                .text_sm()
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if selected {
                    rgb(0x0f172a)
                } else {
                    rgb(0x64748b)
                })
                .child(label),
        )
}

/// 渲染表单标签
fn render_form_label(label: &'static str, icon: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(render_icon(icon, rgb(0x64748b).into()))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x374151))
                .child(label),
        )
}

/// 渲染底部按钮
fn render_footer_buttons(state: Entity<ServerDialogState>) -> impl IntoElement {
    div()
        .h(px(64.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(rgb(0xe2e8f0))
        .flex()
        .items_center()
        .justify_end()
        .gap_3()
        .px_6()
        // 取消按钮
        .child(
            div()
                .id("cancel-btn")
                .px_4()
                .py_2()
                .bg(rgb(0xffffff))
                .border_1()
                .border_color(rgb(0xd1d5db))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0xf3f4f6)))
                .on_click(move |_, _, cx| {
                    state.update(cx, |s, _| s.close());
                })
                .child(div().text_sm().text_color(rgb(0x374151)).child("取消")),
        )
        // 保存按钮
        .child(
            div()
                .id("save-btn")
                .px_4()
                .py_2()
                .bg(rgb(0x3b82f6))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x2563eb)))
                .child(div().text_sm().text_color(rgb(0xffffff)).child("保存")),
        )
}
