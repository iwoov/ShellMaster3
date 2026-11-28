// 服务器弹窗：添加、编辑服务器（基本信息、跳板机、代理）

pub mod helpers;
pub mod panels;

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::input::InputState;

use crate::components::common::button::{text_button, ButtonKind};
use crate::components::common::dialog::{
    dialog_nav_item, dialog_nav_list, dialog_pane_footer, dialog_pane_header, dialog_panel,
    dialog_scroll_area, dialog_sidebar, fit_to_viewport, keep_focus_in_dialog, DialogOverlay,
};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::server::{AuthType, ProxyConfig, ProxyType, ServerData};
use crate::models::settings::Language;
use crate::services::storage;
use crate::ssh::config::resolve_jump_server;

use panels::{render_basic_info_form, render_jump_host_form, render_proxy_settings_form};

/// 弹窗的期望尺寸，窗口较小时随之收缩
const DIALOG_SIZE: Size<Pixels> = size(px(760.), px(560.));
/// 左侧导航宽度
const NAV_WIDTH: Pixels = px(180.);
/// 内容区左右留白
const CONTENT_PADDING_X: Pixels = px(28.);
/// SSH 默认端口：端口留空时使用
const DEFAULT_SSH_PORT: u16 = 22;

/// 左侧导航的分区
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DialogSection {
    #[default]
    BasicInfo,
    JumpHost,
    ProxySettings,
}

impl DialogSection {
    const ALL: [DialogSection; 3] = [
        DialogSection::BasicInfo,
        DialogSection::JumpHost,
        DialogSection::ProxySettings,
    ];

    fn label_key(self) -> &'static str {
        match self {
            DialogSection::BasicInfo => "server_dialog.nav.basic_info",
            DialogSection::JumpHost => "server_dialog.nav.jump_host",
            DialogSection::ProxySettings => "server_dialog.nav.proxy",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            DialogSection::BasicInfo => icons::SERVER,
            DialogSection::JumpHost => icons::LINK,
            DialogSection::ProxySettings => icons::GLOBE,
        }
    }
}

/// 需要校验的字段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Host,
    Port,
    Username,
    PrivateKey,
    JumpHost,
    ProxyHost,
    ProxyPort,
}

impl Field {
    /// 字段所在的分区
    pub fn section(self) -> DialogSection {
        match self {
            Field::Host | Field::Port | Field::Username | Field::PrivateKey => {
                DialogSection::BasicInfo
            }
            Field::JumpHost => DialogSection::JumpHost,
            Field::ProxyHost | Field::ProxyPort => DialogSection::ProxySettings,
        }
    }
}

/// 校验出的问题：字段和提示文字的 i18n key，按字段在表单中的顺序排列
pub type FormErrors = Vec<(Field, &'static str)>;

/// 表单里参与校验的内容
#[derive(Clone, Debug, Default)]
pub struct FormValues {
    pub host: String,
    pub port: String,
    pub username: String,
    pub uses_private_key: bool,
    pub private_key: String,
    pub enable_jump_host: bool,
    /// 已选中跳板机（或保留着无法识别的旧设置）
    pub has_jump_host: bool,
    pub enable_proxy: bool,
    pub proxy_host: String,
    pub proxy_port: String,
}

/// 解析端口：只接受 1–65535 的整数；留空时用 `default`，没有默认值则视为无效
fn parse_port(text: &str, default: Option<u16>) -> Option<u16> {
    let text = text.trim();
    if text.is_empty() {
        return default;
    }
    text.parse::<u16>().ok().filter(|port| *port != 0)
}

/// 校验表单
pub fn validate(values: &FormValues) -> FormErrors {
    let mut errors = FormErrors::new();
    if values.host.trim().is_empty() {
        errors.push((Field::Host, "server_dialog.error.host_required"));
    }
    if parse_port(&values.port, Some(DEFAULT_SSH_PORT)).is_none() {
        errors.push((Field::Port, "server_dialog.error.port_invalid"));
    }
    if values.username.trim().is_empty() {
        errors.push((Field::Username, "server_dialog.error.username_required"));
    }
    if values.uses_private_key && values.private_key.trim().is_empty() {
        errors.push((
            Field::PrivateKey,
            "server_dialog.error.private_key_required",
        ));
    }
    if values.enable_jump_host && !values.has_jump_host {
        errors.push((Field::JumpHost, "server_dialog.error.jump_host_required"));
    }
    if values.enable_proxy {
        if values.proxy_host.trim().is_empty() {
            errors.push((Field::ProxyHost, "server_dialog.error.proxy_host_required"));
        }
        if parse_port(&values.proxy_port, None).is_none() {
            errors.push((Field::ProxyPort, "server_dialog.error.port_invalid"));
        }
    }
    errors
}

/// 可选作跳板机的服务器
#[derive(Clone, Debug)]
pub struct JumpHostCandidate {
    pub id: String,
    pub label: String,
    /// user@host:port
    pub address: String,
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
    /// 界面语言，由调用方在每次渲染前传入
    pub lang: Language,
    // 分组：可直接输入，也可从已有分组中选择
    pub group_input: Option<Entity<InputState>>,
    pub available_groups: Vec<String>,
    /// 待应用到输入框的私钥文件名（由文件选择器设置）
    pub pending_private_key_path: Option<String>,
    /// 导入私钥文件失败时的提示
    pub key_error: Option<String>,
    // 表单 InputState 实体（延迟创建）
    pub label_input: Option<Entity<InputState>>,
    pub host_input: Option<Entity<InputState>>,
    pub port_input: Option<Entity<InputState>>,
    pub username_input: Option<Entity<InputState>>,
    pub password_input: Option<Entity<InputState>>,
    pub description_input: Option<Entity<InputState>>,
    // 认证数据
    pub auth_type: AuthType,
    pub private_key_input: Option<Entity<InputState>>,
    pub passphrase_input: Option<Entity<InputState>>,
    // 跳板机数据：从已保存的服务器中选择
    pub enable_jump_host: bool,
    /// 选中的跳板机服务器 ID
    pub jump_host_id: Option<String>,
    /// 可选作跳板机的服务器（已排除自身及会形成环的服务器）
    pub jump_host_candidates: Vec<JumpHostCandidate>,
    /// 旧版本以 "host:port" 文本保存、且匹配不到已保存服务器的跳板机设置
    pub jump_host_unresolved: Option<String>,
    // 代理数据
    pub enable_proxy: bool,
    pub proxy_type: ProxyType,
    pub proxy_host_input: Option<Entity<InputState>>,
    pub proxy_port_input: Option<Entity<InputState>>,
    pub proxy_username_input: Option<Entity<InputState>>,
    pub proxy_password_input: Option<Entity<InputState>>,
    /// 点过保存且校验没通过之后为 true：此后实时显示校验结果，改对的字段立即去掉错误提示
    show_errors: bool,
    /// 写入配置文件失败时的提示
    save_error: Option<String>,
    focus_handle: Option<FocusHandle>,
    /// 打开后第一次渲染时聚焦第一个输入框
    focus_pending: bool,
    scroll_handle: ScrollHandle,
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
            lang: Language::default(),
            group_input: None,
            available_groups: Vec::new(),
            pending_private_key_path: None,
            key_error: None,
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
            jump_host_id: None,
            jump_host_candidates: Vec::new(),
            jump_host_unresolved: None,
            enable_proxy: false,
            proxy_type: ProxyType::Http,
            proxy_host_input: None,
            proxy_port_input: None,
            proxy_username_input: None,
            proxy_password_input: None,
            show_errors: false,
            save_error: None,
            focus_handle: None,
            focus_pending: false,
            scroll_handle: ScrollHandle::new(),
        }
    }
}

/// 创建一个单行输入框
fn new_input(
    placeholder: &'static str,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<ServerDialogState>,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .masked(masked)
    })
}

/// 设置输入框内容
fn set_input(
    input: &Option<Entity<InputState>>,
    value: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut Context<ServerDialogState>,
) {
    if let Some(input) = input {
        let value = value.into();
        input.update(cx, |state, cx| state.set_value(value, window, cx));
    }
}

/// 读取输入框内容
fn input_text(input: &Option<Entity<InputState>>, cx: &App) -> String {
    input
        .as_ref()
        .map(|input| input.read(cx).text().to_string())
        .unwrap_or_default()
}

/// 去掉首尾空白，结果为空时返回 None
fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

impl ServerDialogState {
    /// 确保输入框已创建（在有 window 上下文时调用），并处理打开时的焦点
    pub fn ensure_inputs_created(
        &mut self,
        lang: &Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.lang = lang.clone();
        let t = |key| i18n::t(lang, key);

        if self.group_input.is_none() {
            self.group_input = Some(new_input(
                t("server_dialog.group_placeholder"),
                false,
                window,
                cx,
            ));
            self.available_groups = storage::get_groups()
                .map(|groups| groups.into_iter().map(|group| group.name).collect())
                .unwrap_or_default();
        }
        if self.label_input.is_none() {
            self.label_input = Some(new_input(
                t("server_dialog.label_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.host_input.is_none() {
            self.host_input = Some(new_input(
                t("server_dialog.host_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.port_input.is_none() {
            self.port_input = Some(new_input("22", false, window, cx));
        }
        if self.username_input.is_none() {
            self.username_input = Some(new_input(
                t("server_dialog.username_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.password_input.is_none() {
            self.password_input = Some(new_input(
                t("server_dialog.password_placeholder"),
                true,
                window,
                cx,
            ));
        }
        if self.description_input.is_none() {
            self.description_input = Some(new_input(
                t("server_dialog.description_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.private_key_input.is_none() {
            self.private_key_input = Some(new_input(
                t("server_dialog.private_key_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.passphrase_input.is_none() {
            self.passphrase_input = Some(new_input(
                t("server_dialog.passphrase_placeholder"),
                true,
                window,
                cx,
            ));
        }
        if self.proxy_host_input.is_none() {
            self.proxy_host_input = Some(new_input(
                t("server_dialog.proxy_host_placeholder"),
                false,
                window,
                cx,
            ));
        }
        if self.proxy_port_input.is_none() {
            // 代理端口没有默认值，不放示例数字，以免被当成默认端口
            self.proxy_port_input = Some(new_input("", false, window, cx));
        }
        if self.proxy_username_input.is_none() {
            self.proxy_username_input = Some(new_input("", false, window, cx));
        }
        if self.proxy_password_input.is_none() {
            self.proxy_password_input = Some(new_input("", true, window, cx));
        }

        // 文件选择器导入私钥后，把文件名填进输入框
        if let Some(path) = self.pending_private_key_path.take() {
            set_input(&self.private_key_input, path, window, cx);
            self.key_error = None;
        }

        if self.pending_load_edit_data {
            self.pending_load_edit_data = false;
            self.load_edit_data(window, cx);
        }

        // 打开时聚焦第一个输入框；之后保证焦点留在弹窗里，Esc 和回车始终有效
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            match &self.label_input {
                Some(input) => input.update(cx, |input, cx| input.focus(window, cx)),
                None => focus_handle.focus(window, cx),
            }
        }
        keep_focus_in_dialog(
            cx.weak_entity(),
            |s: &Self| s.visible,
            &focus_handle,
            window,
        );
    }

    /// 编辑模式：把已保存的服务器填进表单
    fn load_edit_data(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(server_id) = self.edit_server_id.clone() else {
            return;
        };
        let Ok(config) = storage::load_servers() else {
            return;
        };
        let Some(server) = config.servers.iter().find(|s| s.id == server_id) else {
            return;
        };

        if let Some(group) = server
            .group_id
            .as_ref()
            .and_then(|group_id| config.groups.iter().find(|g| &g.id == group_id))
        {
            set_input(&self.group_input, group.name.clone(), window, cx);
        }
        set_input(&self.label_input, server.label.clone(), window, cx);
        set_input(&self.host_input, server.host.clone(), window, cx);
        set_input(&self.port_input, server.port.to_string(), window, cx);
        set_input(&self.username_input, server.username.clone(), window, cx);

        self.auth_type = server.auth_type.clone();
        if let Some(password) = &server.password_encrypted {
            set_input(&self.password_input, password.clone(), window, cx);
        }
        // 优先使用新字段 private_key_filename，不存在时回退到旧字段 private_key_path（向后兼容）
        if let Some(key) = server
            .private_key_filename
            .as_ref()
            .or(server.private_key_path.as_ref())
        {
            set_input(&self.private_key_input, key.clone(), window, cx);
        }
        if let Some(passphrase) = &server.key_passphrase_encrypted {
            set_input(&self.passphrase_input, passphrase.clone(), window, cx);
        }
        if let Some(description) = &server.description {
            set_input(&self.description_input, description.clone(), window, cx);
        }

        // 跳板机设置（兼容旧版本保存的 "host:port" 文本）
        if let Some(reference) = server
            .jump_host_id
            .as_deref()
            .map(str::trim)
            .filter(|reference| !reference.is_empty())
        {
            self.enable_jump_host = true;
            let resolved = resolve_jump_server(reference, &config.servers)
                .map(|server| server.id.clone())
                .filter(|id| {
                    self.jump_host_candidates
                        .iter()
                        .any(|candidate| &candidate.id == id)
                });
            match resolved {
                Some(id) => self.jump_host_id = Some(id),
                None => self.jump_host_unresolved = Some(reference.to_string()),
            }
        }

        if let Some(proxy) = &server.proxy {
            self.enable_proxy = proxy.enabled;
            self.proxy_type = proxy.proxy_type.clone();
            set_input(&self.proxy_host_input, proxy.host.clone(), window, cx);
            if proxy.port != 0 {
                set_input(&self.proxy_port_input, proxy.port.to_string(), window, cx);
            }
            if let Some(username) = &proxy.username {
                set_input(&self.proxy_username_input, username.clone(), window, cx);
            }
            if let Some(password) = &proxy.password_encrypted {
                set_input(&self.proxy_password_input, password.clone(), window, cx);
            }
        }
    }

    pub fn open_add(&mut self) {
        self.reset();
        self.visible = true;
        self.is_edit = false;
        self.edit_server_id = None;
        self.load_jump_host_candidates();
    }

    /// 打开编辑服务器弹窗
    pub fn open_edit(&mut self, server_id: String) {
        self.reset();
        self.visible = true;
        self.is_edit = true;
        self.edit_server_id = Some(server_id);
        self.pending_load_edit_data = true;
        self.load_jump_host_candidates();
    }

    /// 载入可选作跳板机的服务器：排除正在编辑的服务器本身，
    /// 以及跳板机链会绕回它的服务器（避免成环）
    fn load_jump_host_candidates(&mut self) {
        let servers = storage::load_servers()
            .map(|config| config.servers)
            .unwrap_or_default();
        let editing_id = self.edit_server_id.clone();
        self.jump_host_candidates = servers
            .iter()
            .filter(|server| match &editing_id {
                Some(id) => &server.id != id && !jump_chain_reaches(server, id, &servers),
                None => true,
            })
            .map(|server| JumpHostCandidate {
                id: server.id.clone(),
                label: if server.label.trim().is_empty() {
                    server.host.clone()
                } else {
                    server.label.clone()
                },
                address: format!("{}@{}:{}", server.username, server.host, server.port),
            })
            .collect();
    }

    /// 清空表单：输入框在下一次渲染时用当前语言重新创建
    fn reset(&mut self) {
        self.group_input = None;
        self.available_groups.clear();
        self.label_input = None;
        self.host_input = None;
        self.port_input = None;
        self.username_input = None;
        self.password_input = None;
        self.description_input = None;
        self.private_key_input = None;
        self.passphrase_input = None;
        self.proxy_host_input = None;
        self.proxy_port_input = None;
        self.proxy_username_input = None;
        self.proxy_password_input = None;
        self.auth_type = AuthType::Password;
        self.enable_jump_host = false;
        self.jump_host_id = None;
        self.jump_host_candidates.clear();
        self.jump_host_unresolved = None;
        self.enable_proxy = false;
        self.proxy_type = ProxyType::Http;
        self.pending_private_key_path = None;
        self.key_error = None;
        self.show_errors = false;
        self.save_error = None;
        self.focus_pending = true;
        self.current_section = DialogSection::BasicInfo;
        self.scroll_handle.set_offset(Point::default());
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.edit_server_id = None;
    }

    pub fn select_section(&mut self, section: DialogSection) {
        if self.current_section != section {
            self.current_section = section;
            self.scroll_handle.set_offset(Point::default());
        }
    }

    /// 表单里参与校验的内容
    fn form_values(&self, cx: &App) -> FormValues {
        FormValues {
            host: input_text(&self.host_input, cx),
            port: input_text(&self.port_input, cx),
            username: input_text(&self.username_input, cx),
            uses_private_key: self.auth_type == AuthType::PublicKey,
            private_key: input_text(&self.private_key_input, cx),
            enable_jump_host: self.enable_jump_host,
            has_jump_host: self.jump_host_id.is_some() || self.jump_host_unresolved.is_some(),
            enable_proxy: self.enable_proxy,
            proxy_host: input_text(&self.proxy_host_input, cx),
            proxy_port: input_text(&self.proxy_port_input, cx),
        }
    }

    /// 当前要显示的校验错误（还没点过保存时不显示）
    pub fn visible_errors(&self, cx: &App) -> FormErrors {
        if self.show_errors {
            validate(&self.form_values(cx))
        } else {
            FormErrors::new()
        }
    }

    /// 字段对应的输入框（出错时把焦点移过去）
    fn input_for(&self, field: Field) -> Option<&Entity<InputState>> {
        match field {
            Field::Host => self.host_input.as_ref(),
            Field::Port => self.port_input.as_ref(),
            Field::Username => self.username_input.as_ref(),
            Field::PrivateKey => self.private_key_input.as_ref(),
            Field::JumpHost => None,
            Field::ProxyHost => self.proxy_host_input.as_ref(),
            Field::ProxyPort => self.proxy_port_input.as_ref(),
        }
    }

    /// 校验并保存。校验不通过时停留在出错的分区并聚焦出错的输入框；写入失败时显示错误，弹窗不关闭。
    pub fn try_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values = self.form_values(cx);
        let errors = validate(&values);
        if let Some(&(first, _)) = errors.first() {
            self.show_errors = true;
            self.save_error = None;
            // 当前分区有错误就留在这里，否则切到第一个出错的分区
            let section = if errors
                .iter()
                .any(|(field, _)| field.section() == self.current_section)
            {
                self.current_section
            } else {
                first.section()
            };
            self.select_section(section);
            // 必填项都在分区开头，回到顶部就能看到错误
            self.scroll_handle.set_offset(Point::default());
            if let Some(input) = errors
                .iter()
                .find(|(field, _)| field.section() == section)
                .and_then(|(field, _)| self.input_for(*field))
            {
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            cx.notify();
            return;
        }

        let server = self.to_server_data(&values, cx);
        let result = if self.is_edit {
            storage::update_server(server)
        } else {
            storage::add_server(server)
        };
        match result {
            Ok(()) => {
                self.needs_refresh = true;
                self.close();
            }
            Err(err) => {
                self.save_error = Some(format!(
                    "{}{:#}",
                    i18n::t(&self.lang, "server_dialog.save_failed"),
                    err
                ));
            }
        }
        cx.notify();
    }

    /// 由校验通过的表单生成要保存的 ServerData
    fn to_server_data(&self, values: &FormValues, cx: &App) -> ServerData {
        let host = values.host.trim().to_string();
        let label = non_empty(&input_text(&self.label_input, cx)).unwrap_or_else(|| host.clone());
        let port = parse_port(&values.port, Some(DEFAULT_SSH_PORT)).unwrap_or(DEFAULT_SSH_PORT);
        let uses_password = self.auth_type == AuthType::Password;
        let uses_key = self.auth_type == AuthType::PublicKey;
        let password = input_text(&self.password_input, cx);
        let passphrase = input_text(&self.passphrase_input, cx);

        // 分组输入框里填的是名称：找到同名分组就用它的 id，否则用名称作为新分组的 id（保存时创建）
        let group_id = non_empty(&input_text(&self.group_input, cx)).map(|name| {
            storage::get_groups()
                .ok()
                .and_then(|groups| groups.into_iter().find(|g| g.name == name))
                .map(|group| group.id)
                .unwrap_or(name)
        });

        // 编辑时保留原 ID、运行时采集的数据和原始时间信息
        let id = match (&self.edit_server_id, self.is_edit) {
            (Some(id), true) => id.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };
        let existing = if self.is_edit {
            storage::load_servers()
                .ok()
                .and_then(|config| config.servers.into_iter().find(|s| s.id == id))
        } else {
            None
        };

        ServerData {
            id,
            group_id,
            label,
            host,
            port,
            username: values.username.trim().to_string(),
            auth_type: self.auth_type.clone(),
            // 落盘时由 storage::save_servers 使用 AES-256-GCM 自动加密
            password_encrypted: (uses_password && !password.is_empty()).then_some(password),
            private_key_filename: if uses_key {
                non_empty(&values.private_key)
            } else {
                None
            },
            private_key_path: None, // 不再使用完整路径
            key_passphrase_encrypted: (uses_key && !passphrase.is_empty()).then_some(passphrase),
            description: non_empty(&input_text(&self.description_input, cx)),
            jump_host_id: if self.enable_jump_host {
                // 没有重新选择时保留无法识别的旧设置：连接时会明确报错，而不是悄悄直连
                self.jump_host_id
                    .clone()
                    .or_else(|| self.jump_host_unresolved.clone())
            } else {
                None
            },
            proxy: self.enable_proxy.then(|| {
                let password = input_text(&self.proxy_password_input, cx);
                ProxyConfig {
                    enabled: true,
                    proxy_type: self.proxy_type.clone(),
                    host: values.proxy_host.trim().to_string(),
                    port: parse_port(&values.proxy_port, None).unwrap_or_default(),
                    username: non_empty(&input_text(&self.proxy_username_input, cx)),
                    password_encrypted: (!password.is_empty()).then_some(password),
                }
            }),
            enable_monitor: existing
                .as_ref()
                .map(|server| server.enable_monitor)
                .unwrap_or(true),
            os_name: existing.as_ref().and_then(|server| server.os_name.clone()),
            created_at: existing
                .as_ref()
                .map(|server| server.created_at.clone())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            last_connected_at: existing.and_then(|server| server.last_connected_at),
        }
    }
}

/// `server` 的跳板机链（沿 jump_host_id 逐级向外）是否会经过 `target_id`
fn jump_chain_reaches(server: &ServerData, target_id: &str, servers: &[ServerData]) -> bool {
    let mut current = server;
    // 步数有上限：即使已保存的配置里存在环也不会死循环
    for _ in 0..servers.len().min(16) {
        let Some(reference) = current.jump_host_id.as_deref() else {
            return false;
        };
        let Some(next) = resolve_jump_server(reference, servers) else {
            return false;
        };
        if next.id == target_id {
            return true;
        }
        current = next;
    }
    false
}

/// 渲染服务器弹窗覆盖层
pub fn render_server_dialog_overlay(
    state: Entity<ServerDialogState>,
    viewport: Size<Pixels>,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let dialog_size = fit_to_viewport(DIALOG_SIZE, viewport);
    // 右侧表单的可用宽度（去掉左栏、左栏边框和左右留白）
    let form_width = (dialog_size.width - NAV_WIDTH - px(3.) - CONTENT_PADDING_X * 2.).max(px(0.));
    let focus_handle = state.read(cx).focus_handle.clone();

    let state_for_escape = state.clone();
    let state_for_enter = state.clone();
    DialogOverlay::new()
        .focus(focus_handle.as_ref())
        .on_escape(move |_, cx| {
            state_for_escape.update(cx, |s, cx| {
                s.close();
                cx.notify();
            });
        })
        .on_enter(move |window, cx| {
            state_for_enter.update(cx, |s, cx| s.try_save(window, cx));
        })
        .render(
            "server-dialog-overlay",
            dialog_panel(&p)
                .flex_row()
                .w(dialog_size.width)
                .h(dialog_size.height)
                .bg(p.background)
                .child(render_left_nav(state.clone(), &p, cx))
                .child(render_right_content(state, form_width, fit, &p, cx)),
            &p,
        )
}

/// 渲染左侧导航：弹窗标题 + 分区，启用了跳板机、代理的分区右侧显示小圆点
fn render_left_nav(
    state: Entity<ServerDialogState>,
    p: &crate::theme::Palette,
    cx: &App,
) -> impl IntoElement {
    let state_read = state.read(cx);
    let lang = state_read.lang.clone();
    let current = state_read.current_section;
    let (enable_jump_host, enable_proxy) = (state_read.enable_jump_host, state_read.enable_proxy);
    let title = if state_read.is_edit {
        "server_dialog.edit_title"
    } else {
        "server_dialog.add_title"
    };

    dialog_sidebar(i18n::t(&lang, title), NAV_WIDTH, p).child(
        dialog_nav_list("server-dialog-nav").children(DialogSection::ALL.into_iter().map(
            |section| {
                let marked = match section {
                    DialogSection::BasicInfo => false,
                    DialogSection::JumpHost => enable_jump_host,
                    DialogSection::ProxySettings => enable_proxy,
                };
                let state = state.clone();
                dialog_nav_item(
                    SharedString::from(format!("server-nav-{:?}", section)),
                    section.icon(),
                    i18n::t(&lang, section.label_key()),
                    section == current,
                    marked,
                    p,
                )
                .on_click(move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.select_section(section);
                        cx.notify();
                    });
                })
            },
        )),
    )
}

/// 渲染右侧：分区标题、表单和底栏
fn render_right_content(
    state: Entity<ServerDialogState>,
    form_width: Pixels,
    fit: &TextFitter,
    p: &crate::theme::Palette,
    cx: &App,
) -> impl IntoElement {
    let state_read = state.read(cx);
    let lang = state_read.lang.clone();
    let section = state_read.current_section;
    let scroll_handle = state_read.scroll_handle.clone();
    let errors = state_read.visible_errors(cx);
    // 底栏：写入失败的原因，或第一条校验错误
    let message = state_read
        .save_error
        .clone()
        .map(SharedString::from)
        .or_else(|| {
            errors
                .first()
                .map(|(_, key)| SharedString::from(i18n::t(&lang, key)))
        });

    let form = match section {
        DialogSection::BasicInfo => {
            render_basic_info_form(state.clone(), &errors, cx).into_any_element()
        }
        DialogSection::JumpHost => {
            render_jump_host_form(state.clone(), &errors, form_width, fit, cx).into_any_element()
        }
        DialogSection::ProxySettings => {
            render_proxy_settings_form(state.clone(), &errors, cx).into_any_element()
        }
    };

    let state_for_close = state.clone();
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(dialog_pane_header(
            i18n::t(&lang, section.label_key()),
            i18n::t(&lang, "common.close"),
            move |_, cx| {
                state_for_close.update(cx, |s, cx| {
                    s.close();
                    cx.notify();
                });
            },
            CONTENT_PADDING_X,
            p,
        ))
        .child(dialog_scroll_area(
            "server-form-scroll",
            &scroll_handle,
            CONTENT_PADDING_X,
            form,
        ))
        .child(
            dialog_pane_footer(message, CONTENT_PADDING_X, p)
                .child(
                    text_button(
                        "server-cancel-btn",
                        ButtonKind::Secondary,
                        i18n::t(&lang, "common.cancel"),
                        p,
                    )
                    .on_click(move |_, _, cx| {
                        state_for_cancel.update(cx, |s, cx| {
                            s.close();
                            cx.notify();
                        });
                    }),
                )
                .child(
                    text_button(
                        "server-save-btn",
                        ButtonKind::Primary,
                        i18n::t(&lang, "common.save"),
                        p,
                    )
                    .on_click(move |_, window, cx| {
                        state_for_save.update(cx, |s, cx| s.try_save(window, cx));
                    }),
                ),
        )
}

#[cfg(test)]
mod tests {
    // 不用 `use super::*`：会连带引入 gpui 的 `test` 宏，覆盖标准库的 `#[test]`
    use super::{parse_port, validate, DialogSection, Field, FormValues};

    fn valid() -> FormValues {
        FormValues {
            host: "10.0.0.1".into(),
            username: "root".into(),
            ..FormValues::default()
        }
    }

    fn fields(values: &FormValues) -> Vec<Field> {
        validate(values)
            .into_iter()
            .map(|(field, _)| field)
            .collect()
    }

    #[test]
    fn host_and_username_are_required() {
        assert!(validate(&valid()).is_empty());
        let empty = FormValues {
            host: "  ".into(),
            username: String::new(),
            ..valid()
        };
        assert_eq!(fields(&empty), vec![Field::Host, Field::Username]);
    }

    #[test]
    fn ports_must_be_in_range() {
        // 留空时使用默认端口 22
        assert_eq!(parse_port("", Some(22)), Some(22));
        assert_eq!(parse_port(" 2222 ", Some(22)), Some(2222));
        assert_eq!(parse_port("65535", None), Some(65535));
        for invalid in ["0", "65536", "-1", "22a", "1.5"] {
            assert_eq!(parse_port(invalid, Some(22)), None, "{invalid}");
        }
        // 代理端口没有默认值
        assert_eq!(parse_port("", None), None);

        let bad_port = FormValues {
            port: "70000".into(),
            ..valid()
        };
        assert_eq!(fields(&bad_port), vec![Field::Port]);
    }

    #[test]
    fn key_auth_needs_a_key_file() {
        let key_auth = FormValues {
            uses_private_key: true,
            ..valid()
        };
        assert_eq!(fields(&key_auth), vec![Field::PrivateKey]);
        let with_key = FormValues {
            private_key: "1700000000_id_ed25519".into(),
            ..key_auth
        };
        assert!(validate(&with_key).is_empty());
    }

    #[test]
    fn jump_host_and_proxy_are_checked_only_when_enabled() {
        let jump = FormValues {
            enable_jump_host: true,
            ..valid()
        };
        assert_eq!(fields(&jump), vec![Field::JumpHost]);
        assert!(validate(&FormValues {
            has_jump_host: true,
            ..jump
        })
        .is_empty());

        let proxy = FormValues {
            enable_proxy: true,
            ..valid()
        };
        assert_eq!(fields(&proxy), vec![Field::ProxyHost, Field::ProxyPort]);
        assert!(validate(&FormValues {
            proxy_host: "127.0.0.1".into(),
            proxy_port: "1080".into(),
            ..proxy
        })
        .is_empty());

        // 关闭后不再校验
        assert!(validate(&FormValues {
            proxy_host: String::new(),
            ..valid()
        })
        .is_empty());
    }

    #[test]
    fn errors_map_to_their_sections() {
        assert_eq!(Field::PrivateKey.section(), DialogSection::BasicInfo);
        assert_eq!(Field::JumpHost.section(), DialogSection::JumpHost);
        assert_eq!(Field::ProxyPort.section(), DialogSection::ProxySettings);
    }
}
