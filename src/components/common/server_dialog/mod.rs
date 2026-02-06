// 子模块声明
pub mod helpers;
pub mod panels;

use gpui::prelude::*;
use gpui::*;
use gpui_component::input::InputState;
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::server::{AuthType, ProxyConfig, ProxyType, ServerData};
use crate::models::settings::Language;
use crate::services::storage;

use panels::{
    render_basic_info_form, render_jump_host_form, render_other_settings_form,
    render_proxy_settings_form,
};

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
    // 描述
    pub description_input: Option<Entity<InputState>>,
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
            description_input: None,
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
        // 加载当前语言用于占位符文本
        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);

        // 分组输入
        if self.group_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.group_placeholder");
            self.group_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
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
            let placeholder = i18n::t(&lang, "server_dialog.label_placeholder");
            self.label_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.host_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.host_placeholder");
            self.host_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.port_input.is_none() {
            self.port_input = Some(cx.new(|cx| InputState::new(window, cx).placeholder("22")));
        }
        if self.username_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.username");
            self.username_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.password_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.password");
            self.password_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(true)
            }));
        }
        if self.description_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.description_placeholder");
            self.description_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.private_key_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.private_key_placeholder");
            self.private_key_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
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
            let placeholder = i18n::t(&lang, "server_dialog.passphrase");
            self.passphrase_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(true)
            }));
        }

        // 跳板机输入
        if self.jump_host_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.jump_host_placeholder");
            self.jump_host_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }

        // 代理输入
        if self.proxy_host_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.proxy_host");
            self.proxy_host_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.proxy_port_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.proxy_port");
            self.proxy_port_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.proxy_username_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.proxy_username");
            self.proxy_username_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        }
        if self.proxy_password_input.is_none() {
            let placeholder = i18n::t(&lang, "server_dialog.proxy_password");
            self.proxy_password_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
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
                        // 优先使用新字段 private_key_filename，如果不存在则回退到旧字段 private_key_path（向后兼容）
                        let key_to_load = server_data
                            .private_key_filename
                            .as_ref()
                            .or(server_data.private_key_path.as_ref());
                        if let Some(key) = key_to_load {
                            if let Some(input) = &self.private_key_input {
                                input.update(cx, |s, cx| s.set_value(key.clone(), window, cx));
                            }
                        }
                        if let Some(passphrase) = &server_data.key_passphrase_encrypted {
                            if let Some(input) = &self.passphrase_input {
                                input.update(cx, |s, cx| {
                                    s.set_value(passphrase.clone(), window, cx)
                                });
                            }
                        }
                        // 加载描述
                        if let Some(desc) = &server_data.description {
                            if let Some(input) = &self.description_input {
                                input.update(cx, |s, cx| s.set_value(desc.clone(), window, cx));
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
        // 重置所有输入框状态，让它们用当前语言重新创建
        self.reset_inputs();
        self.visible = true;
        self.is_edit = false;
        self.edit_server_id = None;
        self.current_section = DialogSection::BasicInfo;
    }

    /// 打开编辑服务器弹窗
    pub fn open_edit(&mut self, server_id: String) {
        // 重置所有输入框状态，让它们用当前语言重新创建
        self.reset_inputs();
        self.visible = true;
        self.is_edit = true;
        self.edit_server_id = Some(server_id);
        self.pending_load_edit_data = true;
        self.current_section = DialogSection::BasicInfo;
    }

    /// 重置所有输入框状态
    fn reset_inputs(&mut self) {
        self.group_input = None;
        self.label_input = None;
        self.host_input = None;
        self.port_input = None;
        self.username_input = None;
        self.password_input = None;
        self.private_key_input = None;
        self.passphrase_input = None;
        self.jump_host_input = None;
        self.proxy_host_input = None;
        self.proxy_port_input = None;
        self.proxy_username_input = None;
        self.proxy_password_input = None;
        // 重置表单状态
        self.auth_type = AuthType::Password;
        self.enable_jump_host = false;
        self.enable_proxy = false;
        self.proxy_type = ProxyType::Http;
        self.show_group_dropdown = false;
        self.pending_group_value = None;
        self.pending_private_key_path = None;
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
        let description = get_text(&self.description_input);
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
            private_key_filename: if self.auth_type == AuthType::PublicKey
                && !private_key.is_empty()
            {
                Some(private_key)
            } else {
                None
            },
            private_key_path: None, // 不再使用完整路径
            key_passphrase_encrypted: if self.auth_type == AuthType::PublicKey
                && !passphrase.is_empty()
            {
                Some(passphrase) // TODO: 实际应加密
            } else {
                None
            },
            description: if !description.is_empty() {
                Some(description)
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
            // 左侧菜单 180px + 左内边距 24px - 微调 8px
            .left(px(180. + 24. - 8.))
            // 标题栏高度 56px + 表单内边距 + 分组标签和输入框高度
            .top(px(56. + 24. + 24. + 32. + 4.)) // 约 140px
            // 与输入框宽度一致：对话框宽度 700 - 左侧菜单 180 - 左右内边距 48 + 微调 12px
            .w(px(700. - 180. - 48. + 12.))
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
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
                    .bg(cx.theme().popover)
                    .cursor_pointer()
                    .hover(move |s| s.bg(cx.theme().list_hover))
                    .on_click(move |_, _, cx| {
                        state_for_select.update(cx, |s, _| {
                            s.pending_group_value = Some(group_name_for_click.clone());
                            s.show_group_dropdown = false;
                        });
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
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

    // 使用全局主题帮助函数
    let bg_color = crate::theme::popover_color(cx);
    let border_color = cx.theme().border;

    div()
        .id("server-dialog-content")
        .w(px(700.))
        .h(px(500.))
        .bg(bg_color)
        .border_1()
        .border_color(border_color)
        .rounded_lg()
        .shadow_lg()
        .flex()
        .relative() // 添加 relative 使下拉菜单相对于此容器定位
        .overflow_hidden()
        // 阻止鼠标事件传播到底层的遮罩
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        // 阻止滚动事件穿透到底层内容
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_left_menu(state_for_section, cx))
        .child(render_right_content(state, state_for_cancel, cx))
        // 下拉菜单覆盖层 - 在对话框内容最后渲染，确保在最顶层
        .children(render_group_dropdown_overlay(state_for_dropdown, cx))
}

/// 渲染左侧导航菜单
fn render_left_menu(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let sections = [
        (
            DialogSection::BasicInfo,
            i18n::t(&lang, "server_dialog.nav.basic_info"),
            icons::SERVER,
        ),
        (
            DialogSection::JumpHost,
            i18n::t(&lang, "server_dialog.nav.jump_host"),
            icons::LINK,
        ),
        (
            DialogSection::ProxySettings,
            i18n::t(&lang, "server_dialog.nav.proxy"),
            icons::GLOBE,
        ),
        (
            DialogSection::OtherSettings,
            i18n::t(&lang, "server_dialog.nav.other"),
            icons::SETTINGS,
        ),
    ];

    let sidebar_bg = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;
    let hover_bg = cx.theme().muted;
    let icon_color = cx.theme().muted_foreground;
    let text_color = cx.theme().foreground;

    div()
        .w(px(180.))
        .h_full()
        .bg(sidebar_bg)
        .rounded_l_lg() // Ensure left side is rounded
        .border_r_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .p_4()
        .gap_2()
        .children(sections.into_iter().map(|(section, label, icon)| {
            let state = state.clone();
            render_menu_item(
                state, section, label, icon, hover_bg, icon_color, text_color,
            )
        }))
}

/// 渲染单个菜单项
fn render_menu_item(
    state: Entity<ServerDialogState>,
    section: DialogSection,
    label: &'static str,
    icon: &'static str,
    hover_bg: gpui::Hsla,
    icon_color: gpui::Hsla,
    text_color: gpui::Hsla,
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
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            state_for_click.update(cx, |s, _| {
                s.current_section = section;
            });
        })
        .child(render_icon(icon, icon_color.into()))
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染右侧内容区域
fn render_right_content(
    state: Entity<ServerDialogState>,
    state_for_cancel: Entity<ServerDialogState>,
    cx: &App,
) -> impl IntoElement {
    // 加载当前语言和编辑模式
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);
    let is_edit = state.read(cx).is_edit;
    let title = if is_edit {
        i18n::t(&lang, "server_dialog.edit_title")
    } else {
        i18n::t(&lang, "server_dialog.add_title")
    };

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
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .px_6()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child(title),
                ),
        )
        // 表单区域
        .child(
            div()
                .id("form-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scrollbar()
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

/// 渲染底部按钮
fn render_footer_buttons(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);
    let cancel_text = i18n::t(&lang, "common.cancel");
    let save_text = i18n::t(&lang, "common.save");

    let border_color = cx.theme().border;
    let secondary_bg = cx.theme().secondary;
    let secondary_hover = cx.theme().secondary_hover;
    let text_color = cx.theme().foreground;
    let primary_bg = cx.theme().primary;
    let primary_hover = cx.theme().primary_hover;
    let primary_fg = cx.theme().primary_foreground;

    // 提前读取表单数据和编辑模式状态
    let server_data = state.read(cx).to_server_data(cx);
    let is_edit = state.read(cx).is_edit;

    div()
        .h(px(64.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(border_color)
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
                .bg(secondary_bg)
                .border_1()
                .border_color(border_color)
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(secondary_hover))
                .on_click(move |_, _, cx| {
                    state_for_cancel.update(cx, |s, _| s.close());
                })
                .child(div().text_sm().text_color(text_color).child(cancel_text)),
        )
        // 保存按钮
        .child(
            div()
                .id("save-btn")
                .px_4()
                .py_2()
                .bg(primary_bg)
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(primary_hover))
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
                            eprintln!("Failed to save server: {:?}", e);
                        }
                    }
                })
                .child(div().text_sm().text_color(primary_fg).child(save_text)),
        )
}
