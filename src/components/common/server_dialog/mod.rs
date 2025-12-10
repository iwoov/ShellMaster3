use gpui::prelude::*;
use gpui::*;
use gpui_component::input::InputState;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::models::server::{AuthType, ProxyConfig, ProxyType, ServerData};
use crate::services::storage;

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
    /// 编辑模式下的服务器 ID
    pub edit_server_id: Option<String>,
    /// 待加载编辑数据标记（在下一次 ensure_inputs_created 时加载）
    pub pending_load_edit_data: bool,
    pub current_section: DialogSection,
    /// 标记是否需要刷新服务器列表（保存成功后设置为 true）
    pub needs_refresh: bool,
    // 分组选择
    pub group_input: Option<Entity<InputState>>,
    pub show_group_dropdown: bool,
    pub available_groups: Vec<String>,
    /// 待应用到输入框的分组值（由下拉选中时设置）
    pub pending_group_value: Option<String>,
    /// 待应用到输入框的私钥路径（由文件选择器设置）
    pub pending_private_key_path: Option<String>,
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
    // 跳板机数据
    pub enable_jump_host: bool,
    pub jump_host_input: Option<Entity<InputState>>,
    // 代理数据
    pub enable_proxy: bool,
    pub proxy_type: ProxyType,
    pub proxy_host_input: Option<Entity<InputState>>,
    pub proxy_port_input: Option<Entity<InputState>>,
    pub proxy_username_input: Option<Entity<InputState>>,
    pub proxy_password_input: Option<Entity<InputState>>,
}

impl Default for ServerDialogState {
    fn default() -> Self {
        Self {
            visible: false,
            is_edit: false,
            edit_server_id: None,
            pending_load_edit_data: false,
            current_section: DialogSection::BasicInfo,
            needs_refresh: false,
            group_input: None,
            show_group_dropdown: false,
            available_groups: Vec::new(),
            pending_group_value: None,
            pending_private_key_path: None,
            label_input: None,
            host_input: None,
            port_input: None,
            username_input: None,
            password_input: None,
            auth_type: AuthType::Password,
            private_key_input: None,
            passphrase_input: None,
            enable_jump_host: false,
            jump_host_input: None,
            enable_proxy: false,
            proxy_type: ProxyType::Http,
            proxy_host_input: None,
            proxy_port_input: None,
            proxy_username_input: None,
            proxy_password_input: None,
        }
    }
}

impl ServerDialogState {
    /// 确保输入框已创建（在有 window 上下文时调用）
    pub fn ensure_inputs_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 分组输入
        if self.group_input.is_none() {
            self.group_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("选择或输入分组名称")));
            // 加载可用分组
            if let Ok(groups) = storage::get_groups() {
                self.available_groups = groups.into_iter().map(|g| g.name).collect();
            }
        }

        // 应用待设置的分组值
        if let Some(value) = self.pending_group_value.take() {
            if let Some(input) = &self.group_input {
                input.update(cx, |input_state, cx| {
                    input_state.set_value(value, window, cx);
                });
            }
        }
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
            self.private_key_input = Some(
                cx.new(|cx| InputState::new(window, cx).placeholder("点击浏览选择私钥文件...")),
            );
        }
        // 应用待设置的私钥路径
        if let Some(path) = self.pending_private_key_path.take() {
            if let Some(input) = &self.private_key_input {
                input.update(cx, |input_state, cx| {
                    input_state.set_value(path, window, cx);
                });
            }
        }
        if self.passphrase_input.is_none() {
            self.passphrase_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("私钥密码（可选）")
                    .masked(true)
            }));
        }

        // 跳板机输入
        if self.jump_host_input.is_none() {
            self.jump_host_input =
                Some(cx.new(|cx| {
                    InputState::new(window, cx).placeholder("输入跳板机地址 (Host:Port)")
                }));
        }

        // 代理输入
        if self.proxy_host_input.is_none() {
            self.proxy_host_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("代理服务器地址")));
        }
        if self.proxy_port_input.is_none() {
            self.proxy_port_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("端口")));
        }
        if self.proxy_username_input.is_none() {
            self.proxy_username_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("代理用户名 (可选)")));
        }
        if self.proxy_password_input.is_none() {
            self.proxy_password_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("代理密码 (可选)")
                    .masked(true)
            }));
        }

        // 如果是编辑模式且有待加载标记，加载服务器数据
        if self.pending_load_edit_data {
            self.pending_load_edit_data = false;
            if let Some(server_id) = &self.edit_server_id {
                if let Ok(config) = storage::load_servers() {
                    if let Some(server_data) = config.servers.iter().find(|s| &s.id == server_id) {
                        // 加载分组名称
                        if let Some(group_id) = &server_data.group_id {
                            if let Some(group) = config.groups.iter().find(|g| &g.id == group_id) {
                                if let Some(input) = &self.group_input {
                                    input.update(cx, |s, cx| {
                                        s.set_value(group.name.clone(), window, cx)
                                    });
                                }
                            }
                        }
                        // 加载基本信息
                        if let Some(input) = &self.label_input {
                            input.update(cx, |s, cx| {
                                s.set_value(server_data.label.clone(), window, cx)
                            });
                        }
                        if let Some(input) = &self.host_input {
                            input.update(cx, |s, cx| {
                                s.set_value(server_data.host.clone(), window, cx)
                            });
                        }
                        if let Some(input) = &self.port_input {
                            input.update(cx, |s, cx| {
                                s.set_value(server_data.port.to_string(), window, cx)
                            });
                        }
                        if let Some(input) = &self.username_input {
                            input.update(cx, |s, cx| {
                                s.set_value(server_data.username.clone(), window, cx)
                            });
                        }
                        // 设置认证类型
                        self.auth_type = server_data.auth_type.clone();
                        // 加载密码或私钥
                        if let Some(pwd) = &server_data.password_encrypted {
                            if let Some(input) = &self.password_input {
                                input.update(cx, |s, cx| s.set_value(pwd.clone(), window, cx));
                            }
                        }
                        if let Some(key_path) = &server_data.private_key_path {
                            if let Some(input) = &self.private_key_input {
                                input.update(cx, |s, cx| s.set_value(key_path.clone(), window, cx));
                            }
                        }
                        if let Some(passphrase) = &server_data.key_passphrase_encrypted {
                            if let Some(input) = &self.passphrase_input {
                                input.update(cx, |s, cx| {
                                    s.set_value(passphrase.clone(), window, cx)
                                });
                            }
                        }
                        // 加载跳板机设置
                        if let Some(jump_host) = &server_data.jump_host_id {
                            self.enable_jump_host = true;
                            if let Some(input) = &self.jump_host_input {
                                input
                                    .update(cx, |s, cx| s.set_value(jump_host.clone(), window, cx));
                            }
                        }
                        // 加载代理设置
                        if let Some(proxy) = &server_data.proxy {
                            self.enable_proxy = proxy.enabled;
                            self.proxy_type = proxy.proxy_type.clone();
                            if let Some(input) = &self.proxy_host_input {
                                input.update(cx, |s, cx| {
                                    s.set_value(proxy.host.clone(), window, cx)
                                });
                            }
                            if let Some(input) = &self.proxy_port_input {
                                input.update(cx, |s, cx| {
                                    s.set_value(proxy.port.to_string(), window, cx)
                                });
                            }
                            if let Some(username) = &proxy.username {
                                if let Some(input) = &self.proxy_username_input {
                                    input.update(cx, |s, cx| {
                                        s.set_value(username.clone(), window, cx)
                                    });
                                }
                            }
                            if let Some(password) = &proxy.password_encrypted {
                                if let Some(input) = &self.proxy_password_input {
                                    input.update(cx, |s, cx| {
                                        s.set_value(password.clone(), window, cx)
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn open_add(&mut self) {
        self.visible = true;
        self.is_edit = false;
        self.edit_server_id = None;
        self.current_section = DialogSection::BasicInfo;
    }

    /// 打开编辑服务器弹窗
    pub fn open_edit(&mut self, server_id: String) {
        self.visible = true;
        self.is_edit = true;
        self.edit_server_id = Some(server_id);
        self.pending_load_edit_data = true;
        self.current_section = DialogSection::BasicInfo;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.edit_server_id = None;
    }

    /// 从表单状态提取 ServerData
    pub fn to_server_data(&self, cx: &App) -> ServerData {
        let get_text = |input: &Option<Entity<gpui_component::input::InputState>>| -> String {
            input
                .as_ref()
                .map(|i| i.read(cx).text().to_string())
                .unwrap_or_default()
        };

        let group_name = get_text(&self.group_input);
        let label = get_text(&self.label_input);
        let host = get_text(&self.host_input);
        let port_str = get_text(&self.port_input);
        let port = port_str.parse::<u16>().unwrap_or(22);
        let username = get_text(&self.username_input);
        let password = get_text(&self.password_input);
        let private_key = get_text(&self.private_key_input);
        let passphrase = get_text(&self.passphrase_input);
        let jump_host = get_text(&self.jump_host_input);
        let proxy_host = get_text(&self.proxy_host_input);
        let proxy_port_str = get_text(&self.proxy_port_input);
        let proxy_port = proxy_port_str.parse::<u16>().unwrap_or(0);
        let proxy_username = get_text(&self.proxy_username_input);
        let proxy_password = get_text(&self.proxy_password_input);

        // 根据分组名称查找 group_id，如果不存在则使用分组名称作为新 ID
        let group_id = if group_name.is_empty() {
            None
        } else {
            // 尝试从已有分组中查找
            if let Ok(groups) = storage::get_groups() {
                groups
                    .iter()
                    .find(|g| g.name == group_name)
                    .map(|g| g.id.clone())
                    .or_else(|| Some(group_name.clone()))
            } else {
                Some(group_name.clone())
            }
        };

        // 如果是编辑模式，保留原始 ID，否则生成新 ID
        let id = if self.is_edit {
            self.edit_server_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        ServerData {
            id,
            group_id,
            label,
            host,
            port,
            username,
            auth_type: self.auth_type.clone(),
            password_encrypted: if self.auth_type == AuthType::Password && !password.is_empty() {
                Some(password) // TODO: 实际应加密
            } else {
                None
            },
            private_key_path: if self.auth_type == AuthType::PublicKey && !private_key.is_empty() {
                Some(private_key)
            } else {
                None
            },
            key_passphrase_encrypted: if self.auth_type == AuthType::PublicKey
                && !passphrase.is_empty()
            {
                Some(passphrase) // TODO: 实际应加密
            } else {
                None
            },
            jump_host_id: if self.enable_jump_host && !jump_host.is_empty() {
                Some(jump_host)
            } else {
                None
            },
            proxy: if self.enable_proxy {
                Some(ProxyConfig {
                    enabled: true,
                    proxy_type: self.proxy_type.clone(),
                    host: proxy_host,
                    port: proxy_port,
                    username: if !proxy_username.is_empty() {
                        Some(proxy_username)
                    } else {
                        None
                    },
                    password_encrypted: if !proxy_password.is_empty() {
                        Some(proxy_password)
                    } else {
                        None
                    },
                })
            } else {
                None
            },
            enable_monitor: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_connected_at: None,
        }
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

/// 渲染分组下拉菜单覆盖层（在对话框最顶层）
fn render_group_dropdown_overlay(
    state: Entity<ServerDialogState>,
    cx: &App,
) -> Option<impl IntoElement> {
    let state_read = state.read(cx);
    let show_dropdown = state_read.show_group_dropdown;
    let available_groups = state_read.available_groups.clone();

    if !show_dropdown || available_groups.is_empty() {
        return None;
    }

    let state_for_dropdown = state.clone();

    Some(
        div()
            .id("group-dropdown-overlay")
            .absolute()
            // 定位到对话框内分组输入框下方
            // 左侧菜单 180px + 右侧内边距
            .left(px(180. + 24. - 8.))
            // 标题栏高度 56px + 表单内边距 + 分组标签和输入框高度
            .top(px(56. + 24. + 24. + 32. + 4.)) // 约 140px
            // 与输入框宽度一致
            .w(px(700. - 180. - 48. - 32. + 8.))
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xe2e8f0))
            .rounded_md()
            .shadow_lg()
            .max_h(px(200.))
            .children(available_groups.into_iter().map(move |group_name| {
                let state_for_select = state_for_dropdown.clone();
                let group_name_for_display = group_name.clone();
                let group_name_for_click = group_name.clone();
                div()
                    .id(SharedString::from(format!("group-overlay-{}", group_name)))
                    .px_3()
                    .py_2()
                    .bg(rgb(0xffffff))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f5f9)))
                    .on_click(move |_, _, cx| {
                        state_for_select.update(cx, |s, _| {
                            s.pending_group_value = Some(group_name_for_click.clone());
                            s.show_group_dropdown = false;
                        });
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x374151))
                            .child(group_name_for_display),
                    )
            })),
    )
}

/// 渲染弹窗内容
fn render_dialog_content(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    let state_for_section = state.clone();
    let state_for_cancel = state.clone();
    let state_for_dropdown = state.clone();

    div()
        .id("server-dialog-content")
        .w(px(700.))
        .h(px(500.))
        .bg(rgb(0xffffff))
        .rounded_lg()
        .shadow_lg()
        .flex()
        .relative() // 添加 relative 使下拉菜单相对于此容器定位
        .overflow_hidden()
        // 阻止鼠标事件传播到底层的遮罩
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_left_menu(state_for_section))
        .child(render_right_content(state, state_for_cancel, cx))
        // 下拉菜单覆盖层 - 在对话框内容最后渲染，确保在最顶层
        .children(render_group_dropdown_overlay(state_for_dropdown, cx))
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
                .flex_1()
                .overflow_scroll()
                .p_4()
                .child(match state.read(cx).current_section {
                    DialogSection::BasicInfo => {
                        render_basic_info_form(state.clone(), cx).into_any_element()
                    }
                    DialogSection::JumpHost => {
                        render_jump_host_form(state.clone(), cx).into_any_element()
                    }
                    DialogSection::ProxySettings => {
                        render_proxy_settings_form(state.clone(), cx).into_any_element()
                    }
                    DialogSection::OtherSettings => {
                        render_other_settings_form(state.clone(), cx).into_any_element()
                    }
                }),
        )
        // 底部按钮
        .child(render_footer_buttons(state_for_cancel, cx))
}

/// 渲染基本信息表单
fn render_basic_info_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    let state_read = state.read(cx);
    let auth_type = state_read.auth_type.clone();

    // 分组输入框
    let _group_input_entity = state_read.group_input.clone();
    let group_input = if let Some(input) = &state_read.group_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

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
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let private_key_input = if let Some(input) = &state_read.private_key_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };
    let state_for_file_picker = state.clone();

    let passphrase_input = if let Some(input) = &state_read.passphrase_input {
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    let state_for_dropdown_toggle = state.clone();

    div()
        .flex()
        .flex_col()
        .gap_3()
        // 服务器分组（第一个选项）
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .relative()
                .child(render_form_label("服务器分组", icons::FOLDER))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(group_input))
                        .child(
                            // 下拉箭头按钮
                            div()
                                .id("group-dropdown-toggle")
                                .w(px(32.))
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(0xd1d5db))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xf3f4f6)))
                                .on_click(move |_, _, cx| {
                                    state_for_dropdown_toggle.update(cx, |s, _| {
                                        s.show_group_dropdown = !s.show_group_dropdown;
                                    });
                                })
                                .child(render_icon(icons::CHEVRON_DOWN, rgb(0x64748b).into())),
                        ),
                ), // 下拉菜单已移至对话框层级渲染 (render_group_dropdown_overlay)
        )
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
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().flex_1().child(private_key_input))
                                    .child(
                                        // 浏览按钮
                                        div()
                                            .id("browse-private-key-btn")
                                            .px_3()
                                            .py_1p5()
                                            .bg(rgb(0xf3f4f6))
                                            .border_1()
                                            .border_color(rgb(0xd1d5db))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0xe5e7eb)))
                                            .on_click({
                                                let state = state_for_file_picker.clone();
                                                move |_, _, cx| {
                                                    let state = state.clone();
                                                    // 使用 gpui 原生文件选择 API
                                                    let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
                                                        files: true,
                                                        directories: false,
                                                        multiple: false,
                                                        prompt: Some("选择私钥文件".into()),
                                                    });
                                                    cx.spawn(async move |cx| {
                                                        if let Ok(Ok(Some(paths))) = receiver.await {
                                                            if let Some(path) = paths.first() {
                                                                let path_str = path.to_string_lossy().to_string();
                                                                // 设置待应用的私钥路径，下次渲染时会应用
                                                                let _ = cx.update(|app| {
                                                                    state.update(app, |s, _| {
                                                                        s.pending_private_key_path = Some(path_str);
                                                                        s.needs_refresh = true;
                                                                    });
                                                                });
                                                            }
                                                        }
                                                    }).detach();
                                                }
                                            })
                                            .child(render_icon(icons::FOLDER_OPEN, rgb(0x374151).into())),
                                    ),
                            ),
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

/// 渲染跳板机设置表单
fn render_jump_host_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    let state_read = state.read(cx);
    let enabled = state_read.enable_jump_host;

    let jump_host_input = if let Some(input) = &state_read.jump_host_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(render_form_label("启用跳板机", icons::LINK))
                .child({
                    let state = state.clone();
                    render_switch(enabled, move |_, _, cx| {
                        state.update(cx, |s, _| {
                            s.enable_jump_host = !s.enable_jump_host;
                        });
                    })
                }),
        )
        .children(if enabled {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(render_form_label("跳板机地址", icons::SERVER))
                    .child(jump_host_input),
            )
        } else {
            None
        })
}

/// 渲染代理设置表单
fn render_proxy_settings_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use crate::models::server::ProxyType;
    use gpui_component::input::Input;

    let state_read = state.read(cx);
    let enabled = state_read.enable_proxy;
    let proxy_type = state_read.proxy_type.clone();

    let host_input = if let Some(input) = &state_read.proxy_host_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };
    let port_input = if let Some(input) = &state_read.proxy_port_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };
    let username_input = if let Some(input) = &state_read.proxy_username_input {
        Input::new(input).into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };
    let password_input = if let Some(input) = &state_read.proxy_password_input {
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child("加载中...").into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(render_form_label("启用代理", icons::GLOBE))
                .child({
                    let state = state.clone();
                    render_switch(enabled, move |_, _, cx| {
                        state.update(cx, |s, _| {
                            s.enable_proxy = !s.enable_proxy;
                        });
                    })
                }),
        )
        .children(if enabled {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // 代理类型
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label("代理类型", icons::SETTINGS))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .p_1()
                                    .bg(rgb(0xf1f5f9))
                                    .rounded_md()
                                    .child(render_proxy_type_button(
                                        state.clone(),
                                        ProxyType::Http,
                                        "HTTP",
                                        proxy_type == ProxyType::Http,
                                    ))
                                    .child(render_proxy_type_button(
                                        state.clone(),
                                        ProxyType::Socks5,
                                        "SOCKS5",
                                        proxy_type == ProxyType::Socks5,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(render_form_label("主机地址", icons::SERVER))
                                    .child(host_input),
                            )
                            .child(
                                div()
                                    .w(px(100.))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(render_form_label("端口", icons::LINK))
                                    .child(port_input),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label("用户名 (可选)", icons::USER))
                            .child(username_input),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label("密码 (可选)", icons::LOCK))
                            .child(password_input),
                    ),
            )
        } else {
            None
        })
}

/// 渲染其他设置表单
fn render_other_settings_form(_state: Entity<ServerDialogState>, _cx: &App) -> impl IntoElement {
    div().flex().flex_col().gap_3().child(
        div()
            .text_sm()
            .text_color(rgb(0x64748b))
            .child("暂无其他设置选项"),
    )
}

/// 渲染开关组件
fn render_switch(
    checked: bool,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .bg(if checked {
            rgb(0x3b82f6)
        } else {
            rgb(0xe2e8f0)
        })
        .flex()
        .items_center()
        .px(px(2.))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_click)
        .child({
            let thumb = div()
                .size(px(16.))
                .rounded_full()
                .bg(rgb(0xffffff))
                .shadow_sm();
            if checked {
                thumb.ml_auto()
            } else {
                thumb
            }
        })
}

/// 渲染代理类型切换按钮 (复用 render_auth_type_button 逻辑)
fn render_proxy_type_button(
    state: Entity<ServerDialogState>,
    proxy_type: crate::models::server::ProxyType,
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
                s.proxy_type = proxy_type.clone();
            });
        })
        .shadow(if selected {
            vec![BoxShadow {
                color: rgba(0x00000010).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            }]
        } else {
            vec![]
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
fn render_footer_buttons(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    // 提前读取表单数据和编辑模式状态
    let server_data = state.read(cx).to_server_data(cx);
    let is_edit = state.read(cx).is_edit;

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
                    state_for_cancel.update(cx, |s, _| s.close());
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
                .on_click(move |_, _, cx| {
                    // 根据是新增还是编辑模式调用不同的存储函数
                    let result = if is_edit {
                        storage::update_server(server_data.clone())
                    } else {
                        storage::add_server(server_data.clone())
                    };
                    match result {
                        Ok(_) => {
                            // 标记需要刷新，然后关闭弹窗
                            state_for_save.update(cx, |s, _| {
                                s.needs_refresh = true;
                                s.close();
                            });
                        }
                        Err(e) => {
                            eprintln!("保存服务器失败: {:?}", e);
                        }
                    }
                })
                .child(div().text_sm().text_color(rgb(0xffffff)).child("保存")),
        )
}
