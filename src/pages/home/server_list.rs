// 服务器列表组件：列表视图和卡片视图

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme, InteractiveElementExt, Sizable, WindowExt};
use tracing::error;

use super::widgets::{
    grid_cell_width, grid_columns, icon_button, icon_button_colored, page_header, primary_button,
    text_tooltip, TextFitter, ICON_BUTTON_SIZE, SIDEBAR_WIDTH,
};
use crate::components::common::dialog::{render_confirm_dialog, ConfirmDialogText};
use crate::components::common::icon::{
    distribution_icon as detect_distribution_icon, icon_sized, server_icon,
};
use crate::components::common::server_dialog::ServerDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{HistoryItem, Server, ServerGroup};
use crate::services::storage;
use crate::state::SessionState;
use crate::theme::{mix, Palette};

/// 用于联动悬停样式的分组名（GPUI 的分组按名字入栈，同名分组互不干扰）
const ROW_GROUP: &str = "server-row";
const CARD_GROUP: &str = "server-card";
const GROUP_HEADER_GROUP: &str = "server-group-header";
const VIEW_BUTTON_GROUP: &str = "view-button";

/// 内容区窄于这个宽度时，列表不显示描述列
const COMPACT_CONTENT_WIDTH: Pixels = px(780.);
/// 滚动区域的左右留白：只有页面留白（24px）的一半，让行的悬停底色向两侧各多出 12px
const SCROLL_PADDING: Pixels = px(12.);
/// 行内容、卡片再内缩这么多，与页面标题左右对齐
const ITEM_INSET: Pixels = px(12.);

// 列表行
const ROW_HEIGHT: Pixels = px(44.);
const ROW_GAP: Pixels = px(12.);
const ROW_PADDING_RIGHT: Pixels = px(8.);
const ROW_ICON_BOX: Pixels = px(20.);
const ROW_NAME_WIDTH: Pixels = px(180.);
const ROW_TIME_WIDTH: Pixels = px(88.);
/// 描述列和地址列的基准宽度：有富余时在此基础上加宽，不够时按这个比例压缩
const ROW_DESCRIPTION_BASIS: Pixels = px(150.);
const ROW_ADDRESS_BASIS: Pixels = px(300.);
const ROW_ADDRESS_MAX_WIDTH: Pixels = px(440.);
/// 窄窗口下地址列优先拿到的宽度
const ROW_COMPACT_ADDRESS_WIDTH: Pixels = px(250.);

// 卡片
const CARD_GAP: Pixels = px(12.);
/// 卡片的最小宽度：一行能排几张由它决定
const CARD_MIN_WIDTH: Pixels = px(264.);
const CARD_PADDING: Pixels = px(14.);
const CARD_ICON_TILE: f32 = 36.;
const CARD_HEADER_GAP: Pixels = px(11.);
const CARD_ADDRESS_PADDING_LEFT: Pixels = px(10.);
const CARD_ADDRESS_PADDING_RIGHT: Pixels = px(2.);

// 「最近连接」条目
const RECENT_MIN_WIDTH: Pixels = px(196.);
const RECENT_PADDING_LEFT: Pixels = px(10.);
const RECENT_PADDING_RIGHT: Pixels = px(12.);
const RECENT_ICON_TILE: f32 = 32.;
const RECENT_GAP: Pixels = px(10.);

/// 卡片、条目的边框宽度
const BORDER_WIDTH: Pixels = px(1.);
/// 搜索框（小号输入框）的字号
const SEARCH_FONT_SIZE: f32 = 14.;

/// 视图模式
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Card,
}

/// 等待用户确认删除的服务器
pub struct PendingDelete {
    pub server_id: String,
    pub server_name: String,
    /// 确认对话框的焦点，用于响应 Esc
    pub focus_handle: FocusHandle,
}

/// 当前打开的服务器操作菜单（右键菜单和「更多」按钮共用）
struct RowMenu {
    server_id: String,
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    anchor: Anchor,
    _dismiss: Subscription,
}

/// 服务器页的视图状态
#[derive(Default)]
pub struct ViewModeState {
    pub mode: ViewMode,
    pub query: String,
    pub search_input: Option<Entity<InputState>>,
    pub copied_server_id: Option<String>,
    copy_feedback_token: u64,
    /// 已折叠的分组（按分组 id）
    collapsed_groups: HashSet<String>,
    /// 卡片视图下在侧边栏选中的分组；`None` 表示显示全部
    pub group_filter: Option<String>,
    /// 等待用户确认删除的服务器
    pub pending_delete: Option<PendingDelete>,
    /// 当前打开的服务器操作菜单
    row_menu: Option<RowMenu>,
}

impl ViewModeState {
    /// 搜索框依赖 Window 创建，首次进入主页时初始化并订阅实时输入。
    pub fn ensure_search_input_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_input.is_some() {
            return;
        }

        let lang = storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(Language::Chinese);
        let placeholder = i18n::t(&lang, "server_list.search_placeholder");
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));

        cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = input.read(cx).value().trim().to_string();
                cx.notify();
            }
        })
        .detach();

        self.search_input = Some(input);
    }

    /// 侧边栏的分组筛选只在卡片视图生效（列表视图的侧边栏不显示分组）
    pub fn active_group_filter(&self) -> Option<&str> {
        match self.mode {
            ViewMode::Card => self.group_filter.as_deref(),
            ViewMode::List => None,
        }
    }

    /// 点击侧边栏的分组：选中它，再点一次取消
    pub fn toggle_group_filter(&mut self, group_id: &str) {
        if self.group_filter.as_deref() == Some(group_id) {
            self.group_filter = None;
        } else {
            self.group_filter = Some(group_id.to_string());
        }
    }

    fn toggle_group_collapsed(&mut self, group_id: &str) {
        if !self.collapsed_groups.remove(group_id) {
            self.collapsed_groups.insert(group_id.to_string());
        }
    }

    /// 服务器列表重新加载后调用：丢弃已不存在的分组的折叠和筛选状态，
    /// 否则删掉某个分组的最后一台服务器后，筛选会停在一个空分组上。
    pub fn retain_groups(&mut self, groups: &[ServerGroup]) {
        let exists = |id: &str| groups.iter().any(|group| group.id == id);
        self.collapsed_groups.retain(|id| exists(id));
        if self.group_filter.as_deref().is_some_and(|id| !exists(id)) {
            self.group_filter = None;
        }
    }
}

/// 列表视图中随窗口伸缩的几列的宽度
#[derive(Clone, Copy, Debug, PartialEq)]
struct RowColumns {
    name: Pixels,
    /// 窄窗口下不显示描述列
    description: Option<Pixels>,
    address: Pixels,
}

impl RowColumns {
    fn new(row_width: Pixels, compact: bool) -> Self {
        let fixed =
            ITEM_INSET + ROW_PADDING_RIGHT + ROW_ICON_BOX + ROW_TIME_WIDTH + ICON_BUTTON_SIZE;

        if compact {
            // 图标、名称、地址、时间、连接按钮之间有 4 个间距
            let flexible = (row_width - fixed - ROW_GAP * 4.).max(px(0.)).floor();
            // 地址优先拿到固定的宽度，但名称至少占四成
            let name = (flexible - ROW_COMPACT_ADDRESS_WIDTH).max((flexible * 0.4).floor());
            return Self {
                name,
                description: None,
                address: flexible - name,
            };
        }

        // 比窄窗口多出描述一列，共 5 个间距
        let flexible = (row_width - fixed - ROW_NAME_WIDTH - ROW_GAP * 5.)
            .max(px(0.))
            .floor();
        let spare = flexible - ROW_DESCRIPTION_BASIS - ROW_ADDRESS_BASIS;
        let address = if spare >= px(0.) {
            // 有富余时按 2:3 分给描述和地址；地址到上限后，多出来的都给描述
            (ROW_ADDRESS_BASIS + spare * 0.6).min(ROW_ADDRESS_MAX_WIDTH)
        } else {
            // 不够时按基准宽度的比例（1:2）压缩
            flexible * (2. / 3.)
        }
        .floor();

        Self {
            name: ROW_NAME_WIDTH,
            description: Some(flexible - address),
            address,
        }
    }
}

/// 当前窗口宽度下各部分的宽度。
///
/// 文字的可用宽度都从这里算出，再交给 [`TextFitter`] 截断，不依赖布局引擎去量文字。
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    /// 列表行和分组标题的宽度
    row_width: Pixels,
    row: RowColumns,
    card_columns: u16,
    /// 卡片内容区（去掉边框和内边距）的宽度
    card_inner_width: Pixels,
    recent_columns: u16,
    /// 「最近连接」条目里文字区域的宽度
    recent_text_width: Pixels,
}

impl Metrics {
    fn new(content_width: Pixels) -> Self {
        let compact = content_width < COMPACT_CONTENT_WIDTH;
        let row_width = (content_width - SCROLL_PADDING * 2.).max(px(0.));
        let grid_width = row_width - ITEM_INSET * 2.;

        let card_columns = grid_columns(grid_width, CARD_MIN_WIDTH, CARD_GAP);
        let card_width = grid_cell_width(grid_width, card_columns, CARD_GAP);
        let recent_columns = grid_columns(grid_width, RECENT_MIN_WIDTH, CARD_GAP);
        let recent_width = grid_cell_width(grid_width, recent_columns, CARD_GAP);

        Self {
            row_width,
            row: RowColumns::new(row_width, compact),
            card_columns,
            card_inner_width: (card_width - (CARD_PADDING + BORDER_WIDTH) * 2.)
                .max(px(0.))
                .floor(),
            recent_columns,
            recent_text_width: (recent_width
                - BORDER_WIDTH * 2.
                - RECENT_PADDING_LEFT
                - RECENT_PADDING_RIGHT
                - px(RECENT_ICON_TILE)
                - RECENT_GAP)
                .max(px(0.))
                .floor(),
        }
    }
}

/// 渲染各层都要用到的上下文
struct Ctx {
    p: Palette,
    fit: TextFitter,
    m: Metrics,
    lang: Language,
    /// 当前时间，用于把连接时间显示成「3小时前」
    now: chrono::NaiveDateTime,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
}

impl Ctx {
    fn t(&self, key: &'static str) -> &'static str {
        i18n::t(&self.lang, key)
    }

    /// 最近连接时间的相对写法；从未连接过时是对应的提示文字
    fn last_connected(&self, server: &Server) -> String {
        match server.last_connected_at {
            Some(time) => i18n::relative_time(&self.lang, time, self.now),
            None => self.t("server_list.never_connected").to_string(),
        }
    }
}

/// 某一台服务器在当前帧的交互状态
#[derive(Clone, Copy, Default)]
struct ItemState {
    /// 刚复制过地址，复制按钮显示为对勾
    copied: bool,
    /// 它的操作菜单正开着：保持悬停时的样子，让人看得出菜单对应哪一台
    menu_open: bool,
}

/// 打开一个到指定服务器的会话标签
pub(super) fn connect_to_server(
    server_id: &str,
    server_label: &str,
    session_state: &Entity<SessionState>,
    cx: &mut App,
) {
    session_state.update(cx, |state, cx| {
        state.add_tab(server_id.to_string(), server_label.to_string());
        // 确保 Monitor 详情弹窗状态已创建
        state.ensure_monitor_detail_dialog(cx);
    });
}

/// 针对某一台服务器的操作，供按钮、双击和菜单共用
struct ServerActions {
    id: String,
    name: String,
    endpoint: String,
    lang: Language,
    view_state: Entity<ViewModeState>,
    dialog_state: Entity<ServerDialogState>,
    session_state: Entity<SessionState>,
}

impl ServerActions {
    fn new(server: &Server, ctx: &Ctx) -> Rc<Self> {
        Rc::new(Self {
            id: server.id.clone(),
            name: server.name.clone(),
            endpoint: server.endpoint(),
            lang: ctx.lang.clone(),
            view_state: ctx.view_state.clone(),
            dialog_state: ctx.dialog_state.clone(),
            session_state: ctx.session_state.clone(),
        })
    }

    fn connect(&self, cx: &mut App) {
        connect_to_server(&self.id, &self.name, &self.session_state, cx);
    }

    fn copy_address(&self, cx: &mut App) {
        copy_endpoint_with_feedback(
            self.endpoint.clone(),
            self.id.clone(),
            self.view_state.clone(),
            cx,
        );
    }

    fn edit(&self, cx: &mut App) {
        self.dialog_state.update(cx, |state, _| {
            state.open_edit(self.id.clone());
        });
    }

    /// 删除前先弹出确认对话框
    fn request_delete(&self, window: &mut Window, cx: &mut App) {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let pending = PendingDelete {
            server_id: self.id.clone(),
            server_name: self.name.clone(),
            focus_handle,
        };
        self.view_state.update(cx, |state, cx| {
            state.pending_delete = Some(pending);
            cx.notify();
        });
    }

    /// 在指定位置打开这台服务器的操作菜单
    fn open_menu(
        self: &Rc<Self>,
        position: Point<Pixels>,
        anchor: Anchor,
        window: &mut Window,
        cx: &mut App,
    ) {
        let actions = self.clone();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| actions.build_menu(menu));
        // 菜单拿到焦点后才能用方向键选择、用 Esc 关闭
        menu.focus_handle(cx).focus(window, cx);

        let view_state = self.view_state.clone();
        let dismiss = window.subscribe(&menu, cx, move |dismissed, _: &DismissEvent, _, cx| {
            view_state.update(cx, |state, cx| {
                let is_current = state
                    .row_menu
                    .as_ref()
                    .is_some_and(|open| open.menu.entity_id() == dismissed.entity_id());
                if is_current {
                    state.row_menu = None;
                    cx.notify();
                }
            });
        });

        let server_id = self.id.clone();
        self.view_state.update(cx, |state, cx| {
            state.row_menu = Some(RowMenu {
                server_id,
                menu,
                position,
                anchor,
                _dismiss: dismiss,
            });
            cx.notify();
        });
    }

    fn build_menu(self: &Rc<Self>, menu: PopupMenu) -> PopupMenu {
        let item = |key: &'static str, action: fn(&ServerActions, &mut Window, &mut App)| {
            let actions = self.clone();
            PopupMenuItem::new(i18n::t(&self.lang, key))
                .on_click(move |_, window, cx| action(&actions, window, cx))
        };

        menu.item(item("server_list.action.connect", |actions, _, cx| {
            actions.connect(cx)
        }))
        .item(item("server_list.action.copy_address", |actions, _, cx| {
            actions.copy_address(cx)
        }))
        .item(item("common.edit", |actions, _, cx| actions.edit(cx)))
        .separator()
        .item(item("common.delete", |actions, window, cx| {
            actions.request_delete(window, cx)
        }))
    }
}

/// 渲染服务器页所需的数据
pub struct HostsContent<'a> {
    pub groups: &'a [ServerGroup],
    pub history: &'a [HistoryItem],
    pub fit: TextFitter,
    /// 窗口内容区的尺寸，用来决定各列宽度、卡片列数和窄窗口下的取舍
    pub viewport: Size<Pixels>,
    pub lang: Language,
    pub view_state: Entity<ViewModeState>,
    pub dialog_state: Entity<ServerDialogState>,
    pub session_state: Entity<SessionState>,
}

/// 渲染主机内容区域（标题行 + 列表或卡片）
pub fn render_hosts_content(props: HostsContent, cx: &App) -> impl IntoElement {
    let ctx = Ctx {
        p: crate::theme::palette(cx),
        fit: props.fit,
        m: Metrics::new(props.viewport.width - SIDEBAR_WIDTH),
        lang: props.lang,
        now: chrono::Local::now().naive_local(),
        view_state: props.view_state,
        dialog_state: props.dialog_state,
        session_state: props.session_state,
    };
    let p = ctx.p;

    let state = ctx.view_state.read(cx);
    let mode = state.mode;
    let query = state.query.clone();
    let search_input = state.search_input.clone();
    let group_filter = state.active_group_filter().map(str::to_string);
    let copied_server_id = state.copied_server_id.clone();
    let menu_server_id = state.row_menu.as_ref().map(|menu| menu.server_id.clone());
    let open_menu = state
        .row_menu
        .as_ref()
        .map(|menu| (menu.menu.clone(), menu.position, menu.anchor));
    // 搜索时忽略折叠状态，否则匹配到的服务器可能藏在折叠的分组里
    let collapsed_groups = if query.is_empty() {
        state.collapsed_groups.clone()
    } else {
        HashSet::new()
    };

    let total_servers = props.groups.iter().map(|g| g.servers.len()).sum::<usize>();
    let visible_groups = visible_groups(props.groups, group_filter.as_deref(), &query);
    let visible_servers = visible_groups
        .iter()
        .map(|g| g.servers.len())
        .sum::<usize>();
    let narrowed = group_filter.is_some() || !query.is_empty();
    let count = (total_servers > 0).then(|| {
        SharedString::from(if narrowed {
            format!("{} / {}", visible_servers, total_servers)
        } else {
            total_servers.to_string()
        })
    });

    let item_state = |server: &Server| ItemState {
        copied: copied_server_id.as_deref() == Some(server.id.as_str()),
        menu_open: menu_server_id.as_deref() == Some(server.id.as_str()),
    };

    let body = if total_servers == 0 {
        render_empty_state(&ctx).into_any_element()
    } else if visible_servers == 0 {
        render_search_empty(&query, &ctx).into_any_element()
    } else {
        let view = match mode {
            ViewMode::List => {
                render_list_view(&visible_groups, &collapsed_groups, &item_state, &ctx)
            }
            ViewMode::Card => render_card_view(
                &visible_groups,
                // 「最近连接」不属于搜索结果，也不属于某个分组：搜索或选中分组时不显示
                if narrowed { &[] } else { props.history },
                &collapsed_groups,
                &item_state,
                &ctx,
            ),
        };

        div()
            .id("server-list-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_x_hidden()
            .overflow_y_scrollbar()
            .px(SCROLL_PADDING)
            .pb(px(20.))
            .child(view)
            .into_any_element()
    };

    div()
        .size_full()
        .overflow_hidden()
        .bg(p.background)
        .text_size(px(13.))
        .flex()
        .flex_col()
        .child(render_header(count, mode, search_input, &ctx))
        .child(body)
        .children(open_menu.map(|(menu, position, anchor)| {
            render_menu_layer(menu, position, anchor, props.viewport)
        }))
}

/// 取出要展示的分组：先按侧边栏选中的分组收窄，再按搜索词过滤
fn visible_groups(
    groups: &[ServerGroup],
    group_filter: Option<&str>,
    query: &str,
) -> Vec<ServerGroup> {
    match group_filter {
        Some(id) => {
            let scoped: Vec<ServerGroup> = groups
                .iter()
                .filter(|group| group.id == id)
                .cloned()
                .collect();
            filter_server_groups(&scoped, query)
        }
        None => filter_server_groups(groups, query),
    }
}

fn filter_server_groups(server_groups: &[ServerGroup], query: &str) -> Vec<ServerGroup> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return server_groups.to_vec();
    }

    server_groups
        .iter()
        .filter_map(|group| {
            let group_matches = group.name.to_lowercase().contains(&query);
            let servers = group
                .servers
                .iter()
                .filter(|server| {
                    group_matches
                        || server.name.to_lowercase().contains(&query)
                        || server.host.to_lowercase().contains(&query)
                        || server.account.to_lowercase().contains(&query)
                        || server.description.to_lowercase().contains(&query)
                        || server.port.to_string().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>();

            (!servers.is_empty()).then(|| ServerGroup {
                id: group.id.clone(),
                name: group.name.clone(),
                servers,
            })
        })
        .collect()
}

/// 渲染标题行：标题、数量、搜索框、视图切换和添加按钮
fn render_header(
    count: Option<SharedString>,
    mode: ViewMode,
    search_input: Option<Entity<InputState>>,
    ctx: &Ctx,
) -> impl IntoElement {
    let p = ctx.p;
    let dialog_state = ctx.dialog_state.clone();
    // 搜索框默认 264px；占位文字更长时（英文）加宽到能完整显示，最宽 340px。
    // 56px 是框内图标、间距和输入框自身留白的总和
    let placeholder_width = ctx.fit.width(
        ctx.t("server_list.search_placeholder"),
        SEARCH_FONT_SIZE,
        FontWeight::NORMAL,
    );
    let search_width = (placeholder_width + px(56.)).max(px(264.)).min(px(340.));

    page_header(ctx.t("sidebar.hosts"), count, &p)
        .children(search_input.map(|input| {
            // 窗口变窄时让出空间，最窄 140px
            div()
                .w(search_width)
                .min_w(px(140.))
                .flex_shrink_1()
                .h(px(30.))
                .pl(px(9.))
                .rounded(px(7.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .flex()
                .items_center()
                .child(icon_sized(icons::SEARCH, 14., p.text_faint))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&input).appearance(false).small()),
                )
        }))
        .child(
            div()
                .flex_none()
                .h(px(30.))
                .p(px(2.))
                .rounded(px(7.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .flex()
                .items_center()
                .gap(px(2.))
                .child(render_view_button(
                    "view-list-btn",
                    icons::LIST,
                    ViewMode::List,
                    mode,
                    "server_list.view.list",
                    ctx,
                ))
                .child(render_view_button(
                    "view-card-btn",
                    icons::GRID,
                    ViewMode::Card,
                    mode,
                    "server_list.view.card",
                    ctx,
                )),
        )
        .child({
            let dialog_for_import = dialog_state.clone();
            let lang_for_import = ctx.lang.clone();
            icon_button(
                "import-ssh-config-btn",
                icons::DOWNLOAD,
                ctx.t("server_list.import_ssh_config"),
                &p,
            )
            .on_click(move |_, window, cx| {
                let (message, kind) =
                    match crate::services::ssh_config_import::import_default_ssh_config() {
                        Ok(Some(summary)) if summary.imported > 0 => {
                            dialog_for_import.update(cx, |s, cx| {
                                s.needs_refresh = true;
                                cx.notify();
                            });
                            (
                                i18n::t(&lang_for_import, "server_list.import_ssh_config_result")
                                    .replace("{imported}", &summary.imported.to_string())
                                    .replace("{skipped}", &summary.skipped.to_string()),
                                NotificationType::Success,
                            )
                        }
                        Ok(Some(_)) => (
                            i18n::t(&lang_for_import, "server_list.import_ssh_config_empty")
                                .to_string(),
                            NotificationType::Info,
                        ),
                        Ok(None) => (
                            i18n::t(&lang_for_import, "server_list.import_ssh_config_missing")
                                .to_string(),
                            NotificationType::Warning,
                        ),
                        Err(e) => (
                            format!(
                                "{}: {}",
                                i18n::t(&lang_for_import, "server_list.import_ssh_config_failed"),
                                e
                            ),
                            NotificationType::Error,
                        ),
                    };
                window.push_notification(Notification::new().message(message).with_type(kind), cx);
            })
        })
        .child(
            primary_button(
                "add-server-btn",
                icons::PLUS,
                ctx.t("server_list.add_server"),
                &p,
            )
            .on_click(move |_, _, cx| {
                dialog_state.update(cx, |s, _| s.open_add());
            }),
        )
}

fn render_view_button(
    id: &'static str,
    icon: &'static str,
    target: ViewMode,
    current: ViewMode,
    tooltip_key: &'static str,
    ctx: &Ctx,
) -> impl IntoElement {
    let p = ctx.p;
    let selected = target == current;
    let view_state = ctx.view_state.clone();

    div()
        .id(id)
        .group(VIEW_BUTTON_GROUP)
        .w(px(26.))
        .h(px(24.))
        .rounded(px(5.))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |button| button.bg(p.hover_strong))
        .tooltip(text_tooltip(ctx.t(tooltip_key)))
        .on_click(move |_, _, cx| {
            view_state.update(cx, |state, cx| {
                state.mode = target;
                cx.notify();
            });
        })
        .child(
            icon_sized(icon, 14., if selected { p.text } else { p.text_faint })
                .group_hover(VIEW_BUTTON_GROUP, move |s| s.text_color(p.text)),
        )
}

/// 渲染列表视图：每组一行小标题，下面是单行的服务器
fn render_list_view(
    groups: &[ServerGroup],
    collapsed_groups: &HashSet<String>,
    item_state: &dyn Fn(&Server) -> ItemState,
    ctx: &Ctx,
) -> Div {
    let mut list = div().flex().flex_col();
    for (index, group) in groups.iter().enumerate() {
        let collapsed = collapsed_groups.contains(&group.id);
        list = list.child(render_group_header(group, collapsed, index == 0, ctx));
        if !collapsed {
            list = list.children(
                group
                    .servers
                    .iter()
                    .map(|server| render_server_row(server, item_state(server), ctx)),
            );
        }
    }
    list
}

/// 渲染卡片视图：顶部一排「最近连接」，下面按分组排卡片
fn render_card_view(
    groups: &[ServerGroup],
    history: &[HistoryItem],
    collapsed_groups: &HashSet<String>,
    item_state: &dyn Fn(&Server) -> ItemState,
    ctx: &Ctx,
) -> Div {
    // 「最近连接」只排一行，排不下的不显示
    let recent: Vec<&HistoryItem> = history.iter().take(ctx.m.recent_columns as usize).collect();
    let mut view = div().flex().flex_col();

    if !recent.is_empty() {
        view = view
            .child(render_section_title(
                ctx.t("sidebar.recent"),
                recent.len(),
                ctx,
            ))
            .child(
                div()
                    .grid()
                    .grid_cols(ctx.m.recent_columns)
                    .gap(CARD_GAP)
                    .px(ITEM_INSET)
                    .pt(px(2.))
                    .pb(px(6.))
                    .children(recent.iter().map(|item| render_recent_tile(item, ctx))),
            );
    }

    for (index, group) in groups.iter().enumerate() {
        let collapsed = collapsed_groups.contains(&group.id);
        let first = index == 0 && recent.is_empty();
        view = view.child(render_group_header(group, collapsed, first, ctx));
        if !collapsed {
            view = view.child(
                div()
                    .grid()
                    .grid_cols(ctx.m.card_columns)
                    .gap(CARD_GAP)
                    .px(ITEM_INSET)
                    .pt(px(2.))
                    .pb(px(4.))
                    .children(
                        group
                            .servers
                            .iter()
                            .map(|server| render_server_card(server, item_state(server), ctx)),
                    ),
            );
        }
    }
    view
}

/// 分组标题的外框：一行小字，后面跟一条细线
fn section_title_row(first: bool) -> Div {
    div()
        .h(px(30.))
        .px(ITEM_INSET)
        .when(!first, |row| row.mt(px(10.)))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
}

/// 分组标题后面的数量和细线
fn section_title_tail<E: ParentElement>(row: E, count: usize, p: &Palette) -> E {
    row.child(
        div()
            .flex_none()
            .text_color(p.text_faint)
            .child(count.to_string()),
    )
    .child(div().flex_1().ml(px(6.)).h(px(1.)).bg(p.line))
}

/// 渲染不可折叠的小标题（「最近连接」），它总是排在最前面
fn render_section_title(title: &'static str, count: usize, ctx: &Ctx) -> impl IntoElement {
    let p = ctx.p;
    section_title_tail(
        section_title_row(true).child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text_muted)
                .child(title),
        ),
        count,
        &p,
    )
}

/// 渲染分组标题：点击折叠或展开
fn render_group_header(
    group: &ServerGroup,
    collapsed: bool,
    first: bool,
    ctx: &Ctx,
) -> impl IntoElement {
    let p = ctx.p;
    let view_state = ctx.view_state.clone();
    let group_id = group.id.clone();
    // 给箭头、数量和后面的细线留出位置
    let name_width = (ctx.m.row_width - px(140.)).max(px(60.));

    let row = section_title_row(first)
        .id(SharedString::from(format!("group-header-{}", group.id)))
        .group(GROUP_HEADER_GROUP)
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            view_state.update(cx, |state, cx| {
                state.toggle_group_collapsed(&group_id);
                cx.notify();
            });
        })
        .child(
            icon_sized(
                if collapsed {
                    icons::CHEVRON_RIGHT
                } else {
                    icons::CHEVRON_DOWN
                },
                12.,
                p.text_faint,
            )
            .group_hover(GROUP_HEADER_GROUP, move |s| s.text_color(p.text)),
        )
        .child(
            ctx.fit
                .label(&group.name, name_width, 12., FontWeight::SEMIBOLD)
                .flex_none()
                .text_color(p.text_muted)
                .group_hover(GROUP_HEADER_GROUP, move |s| s.text_color(p.text)),
        );

    section_title_tail(row, group.servers.len(), &p)
}

/// 渲染宽度为 `width` 的 `user@host:port`：账号和端口用弱色，主机名过长时在主机名末尾截断，
/// 端口始终可见。有内容被省略时，鼠标悬停显示完整地址。
fn render_address(id: SharedString, server: &Server, width: Pixels, ctx: &Ctx) -> Stateful<Div> {
    const SIZE: f32 = 12.;
    /// 主机名至少要有这么宽才分段显示
    const MIN_HOST_WIDTH: Pixels = px(48.);

    let p = ctx.p;
    let user = format!("{}@", server.account);
    let port = format!(":{}", server.port);
    let host_width = width - ctx.fit.mono_width(&user, SIZE) - ctx.fit.mono_width(&port, SIZE);
    let address = ctx
        .fit
        .mono_container(SIZE)
        .id(id)
        .flex_none()
        .w(width)
        .flex()
        .items_center();
    let with_tooltip = |address: Stateful<Div>, truncated: bool| {
        address.when(truncated, |address| {
            address.tooltip(text_tooltip(server.endpoint()))
        })
    };

    if host_width < MIN_HOST_WIDTH {
        // 账号太长，主机名几乎没有位置：不再分段，整体截断
        let fitted = ctx.fit.fit_mono(&server.endpoint(), width, SIZE);
        return with_tooltip(address, fitted.truncated)
            .text_color(p.text_soft)
            .child(fitted.text);
    }

    let host = ctx.fit.fit_mono(&server.host, host_width, SIZE);
    with_tooltip(address, host.truncated)
        .child(div().flex_none().text_color(p.text_faint).child(user))
        .child(div().flex_none().text_color(p.text_soft).child(host.text))
        .child(div().flex_none().text_color(p.text_faint).child(port))
}

/// 服务器的图标及颜色：识别出发行版时用品牌图标，否则用通用图标
fn server_icon_for(server: &Server, p: &Palette) -> (&'static str, Hsla) {
    server_icon(distribution_icon(server), p.is_dark, p.text_faint)
}

/// 「复制地址」按钮：刚复制过时显示为绿色对勾
fn render_copy_button(
    id: SharedString,
    state: ItemState,
    actions: &Rc<ServerActions>,
    ctx: &Ctx,
) -> Stateful<Div> {
    let p = ctx.p;
    let tooltip = ctx.t("server_list.action.copy_address");
    let button = if state.copied {
        icon_button_colored(id, icons::CHECK, p.success, p.success, tooltip, &p)
    } else {
        icon_button(id, icons::COPY, tooltip, &p)
    };
    let actions = actions.clone();
    button.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        cx.stop_propagation();
        actions.copy_address(cx);
    })
}

/// 「更多」按钮：在按钮下方打开操作菜单
fn render_more_button(id: SharedString, actions: &Rc<ServerActions>, ctx: &Ctx) -> Stateful<Div> {
    let actions = actions.clone();
    icon_button(
        id,
        icons::ELLIPSIS,
        ctx.t("server_list.action.more"),
        &ctx.p,
    )
    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
        cx.stop_propagation();
        // 菜单的右上角落在光标的右下方，展开后不会盖住按钮
        let position = point(event.position.x + px(8.), event.position.y + px(12.));
        actions.open_menu(position, Anchor::TopRight, window, cx);
    })
}

/// 渲染列表视图中的一行
fn render_server_row(server: &Server, state: ItemState, ctx: &Ctx) -> impl IntoElement {
    let p = ctx.p;
    let columns = ctx.m.row;
    let actions = ServerActions::new(server, ctx);
    let (icon, icon_color) = server_icon_for(server, &p);
    // 菜单开着时保持悬停时的样子
    let active = state.menu_open;
    let connect_tint = p.primary.opacity(0.14);
    let connect_hover = p.primary.opacity(0.24);

    div()
        .id(SharedString::from(format!("row-{}", server.id)))
        .group(ROW_GROUP)
        .relative()
        .h(ROW_HEIGHT)
        .pl(ITEM_INSET)
        .pr(ROW_PADDING_RIGHT)
        .rounded(px(8.))
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .cursor_pointer()
        .when(active, |row| row.bg(p.hover))
        .hover(move |s| s.bg(p.hover))
        // 双击连接服务器
        .on_double_click({
            let actions = actions.clone();
            move |_, _, cx| actions.connect(cx)
        })
        .on_mouse_down(MouseButton::Right, {
            let actions = actions.clone();
            move |event, window, cx| {
                cx.stop_propagation();
                actions.open_menu(event.position, Anchor::TopLeft, window, cx);
            }
        })
        .child(
            div()
                .flex_none()
                .size(ROW_ICON_BOX)
                .flex()
                .items_center()
                .justify_center()
                .child(icon_sized(icon, 18., icon_color)),
        )
        .child(
            ctx.fit
                .label_with_tooltip(
                    SharedString::from(format!("row-name-{}", server.id)),
                    &server.name,
                    columns.name,
                    13.,
                    FontWeight::MEDIUM,
                )
                .flex_none()
                .w(columns.name)
                .text_color(p.text),
        )
        .children(columns.description.map(|width| {
            ctx.fit
                .label_with_tooltip(
                    SharedString::from(format!("row-description-{}", server.id)),
                    &server.description,
                    width,
                    13.,
                    FontWeight::NORMAL,
                )
                .flex_none()
                .w(width)
                .text_color(p.text_muted)
        }))
        .child(render_address(
            SharedString::from(format!("row-address-{}", server.id)),
            server,
            columns.address,
            ctx,
        ))
        // 最近连接时间：悬停时让位给操作按钮
        .child(
            div()
                .flex_none()
                .w(ROW_TIME_WIDTH)
                .text_right()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .when(active, |time| time.invisible())
                .group_hover(ROW_GROUP, |s| s.invisible())
                .child(ctx.last_connected(server)),
        )
        // 悬停时出现的操作按钮，叠在时间的位置上
        .child(
            div()
                .absolute()
                .top((ROW_HEIGHT - ICON_BUTTON_SIZE) / 2.)
                .right(ROW_PADDING_RIGHT + ICON_BUTTON_SIZE + px(8.))
                .flex()
                .gap(px(2.))
                .when(!active, |buttons| buttons.invisible())
                .group_hover(ROW_GROUP, |s| s.visible())
                .child(render_copy_button(
                    SharedString::from(format!("row-copy-{}", server.id)),
                    state,
                    &actions,
                    ctx,
                ))
                .child(
                    icon_button(
                        SharedString::from(format!("row-edit-{}", server.id)),
                        icons::EDIT,
                        ctx.t("common.edit"),
                        &p,
                    )
                    .on_mouse_down(MouseButton::Left, {
                        let actions = actions.clone();
                        move |_, _, cx| {
                            cx.stop_propagation();
                            actions.edit(cx);
                        }
                    }),
                )
                .child(render_more_button(
                    SharedString::from(format!("row-more-{}", server.id)),
                    &actions,
                    ctx,
                )),
        )
        // 连接按钮：平时只是一个弱色图标，悬停该行时提亮
        .child(
            div()
                .id(SharedString::from(format!("row-connect-{}", server.id)))
                .flex_none()
                .size(ICON_BUTTON_SIZE)
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .when(active, |button| button.bg(connect_tint))
                .group_hover(ROW_GROUP, move |s| s.bg(connect_tint))
                .hover(move |s| s.bg(connect_hover))
                .tooltip(text_tooltip(ctx.t("server_list.action.connect")))
                .on_mouse_down(MouseButton::Left, {
                    let actions = actions.clone();
                    move |_, _, cx| {
                        cx.stop_propagation();
                        actions.connect(cx);
                    }
                })
                .child(
                    icon_sized(
                        icons::PLAY,
                        14.,
                        if active { p.primary } else { p.text_faint },
                    )
                    .group_hover(ROW_GROUP, move |s| s.text_color(p.primary)),
                ),
        )
}

/// 带底色的图标块（卡片和「最近连接」条目用）
fn render_icon_tile(icon: &'static str, color: Hsla, size: f32, icon_size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(color.opacity(0.14))
        .flex()
        .items_center()
        .justify_center()
        .child(icon_sized(icon, icon_size, color))
}

/// 渲染卡片视图中的一张卡片
fn render_server_card(server: &Server, state: ItemState, ctx: &Ctx) -> impl IntoElement {
    let p = ctx.p;
    let actions = ServerActions::new(server, ctx);
    let (icon, icon_color) = server_icon_for(server, &p);
    let active = state.menu_open;
    let active_border = mix(p.border, p.primary, 0.55);
    let has_description = !server.description.trim().is_empty();
    let last_connected_tooltip = server.last_connected_at.map(|time| {
        ctx.t("server_list.last_connected_at")
            .replace("{}", &time.format("%Y-%m-%d %H:%M").to_string())
    });

    let inner_width = ctx.m.card_inner_width;
    // 图标块和「更多」按钮之间的名称、描述
    let title_width =
        (inner_width - px(CARD_ICON_TILE) - CARD_HEADER_GAP * 2. - ICON_BUTTON_SIZE).max(px(0.));
    // 地址条里复制按钮左边的地址
    let address_width =
        (inner_width - CARD_ADDRESS_PADDING_LEFT - CARD_ADDRESS_PADDING_RIGHT - ICON_BUTTON_SIZE)
            .max(px(0.));

    div()
        .id(SharedString::from(format!("card-{}", server.id)))
        .group(CARD_GROUP)
        .min_w_0()
        .p(CARD_PADDING)
        .rounded(px(12.))
        .bg(p.surface)
        .border_1()
        .border_color(if active { active_border } else { p.border })
        .when(!p.is_dark, |card| card.shadow_xs())
        .flex()
        .flex_col()
        .gap(px(12.))
        .cursor_pointer()
        .hover(move |s| s.border_color(active_border))
        // 双击连接服务器
        .on_double_click({
            let actions = actions.clone();
            move |_, _, cx| actions.connect(cx)
        })
        .on_mouse_down(MouseButton::Right, {
            let actions = actions.clone();
            move |event, window, cx| {
                cx.stop_propagation();
                actions.open_menu(event.position, Anchor::TopLeft, window, cx);
            }
        })
        // 图标、名称和描述
        .child(
            div()
                .h(px(38.))
                .flex()
                .items_center()
                .gap(CARD_HEADER_GAP)
                .child(render_icon_tile(icon, icon_color, CARD_ICON_TILE, 20.))
                .child(
                    div()
                        .flex_none()
                        .w(title_width)
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(1.))
                        .child(
                            ctx.fit
                                .label_with_tooltip(
                                    SharedString::from(format!("card-name-{}", server.id)),
                                    &server.name,
                                    title_width,
                                    14.,
                                    FontWeight::SEMIBOLD,
                                )
                                .line_height(px(19.))
                                .text_color(p.text),
                        )
                        .children(has_description.then(|| {
                            ctx.fit
                                .label_with_tooltip(
                                    SharedString::from(format!("card-description-{}", server.id)),
                                    &server.description,
                                    title_width,
                                    12.,
                                    FontWeight::NORMAL,
                                )
                                .line_height(px(17.))
                                .text_color(p.text_muted)
                        })),
                )
                .child(
                    render_more_button(
                        SharedString::from(format!("card-more-{}", server.id)),
                        &actions,
                        ctx,
                    )
                    .when(!active, |button| button.invisible())
                    .group_hover(CARD_GROUP, |s| s.visible()),
                ),
        )
        // 连接地址
        .child(
            div()
                .h(px(32.))
                .pl(CARD_ADDRESS_PADDING_LEFT)
                .pr(CARD_ADDRESS_PADDING_RIGHT)
                .rounded(px(7.))
                .bg(p.inset)
                .flex()
                .items_center()
                .child(render_address(
                    SharedString::from(format!("card-address-{}", server.id)),
                    server,
                    address_width,
                    ctx,
                ))
                .child(
                    render_copy_button(
                        SharedString::from(format!("card-copy-{}", server.id)),
                        state,
                        &actions,
                        ctx,
                    )
                    .when(!active, |button| button.invisible())
                    .group_hover(CARD_GROUP, |s| s.visible()),
                ),
        )
        // 最近连接时间和连接按钮
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .id(SharedString::from(format!("card-time-{}", server.id)))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(icon_sized(icons::HISTORY, 12., p.text_faint))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_size(px(12.))
                                .text_color(p.text_faint)
                                .child(ctx.last_connected(server)),
                        )
                        .when_some(last_connected_tooltip, |time, tooltip| {
                            time.tooltip(text_tooltip(tooltip))
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("card-connect-{}", server.id)))
                        .flex_none()
                        .h(px(28.))
                        .pl(px(9.))
                        .pr(px(11.))
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .bg(if active { p.primary } else { p.muted })
                        .group_hover(CARD_GROUP, move |s| s.bg(p.primary))
                        .hover(move |s| s.bg(p.primary_hover))
                        .on_mouse_down(MouseButton::Left, {
                            let actions = actions.clone();
                            move |_, _, cx| {
                                cx.stop_propagation();
                                actions.connect(cx);
                            }
                        })
                        .child(
                            icon_sized(
                                icons::PLAY,
                                12.,
                                if active { p.primary_foreground } else { p.text },
                            )
                            .group_hover(CARD_GROUP, move |s| s.text_color(p.primary_foreground)),
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if active { p.primary_foreground } else { p.text })
                                .group_hover(CARD_GROUP, move |s| {
                                    s.text_color(p.primary_foreground)
                                })
                                .child(ctx.t("server_list.action.connect")),
                        ),
                ),
        )
}

/// 渲染「最近连接」的一个条目，点击直接连接
fn render_recent_tile(item: &HistoryItem, ctx: &Ctx) -> impl IntoElement {
    const TIME_SIZE: f32 = 11.;

    let p = ctx.p;
    let (icon, icon_color) = server_icon(item.distro_icon, p.is_dark, p.text_faint);
    let hover_border = mix(p.border, p.primary, 0.55);
    let session_state = ctx.session_state.clone();
    let (server_id, server_name) = (item.server_id.clone(), item.name.clone());

    let time = i18n::relative_time(&ctx.lang, item.connected_at, ctx.now);
    let text_width = ctx.m.recent_text_width;
    // 名称让出时间和两者之间的间距
    let name_width =
        (text_width - ctx.fit.width(&time, TIME_SIZE, FontWeight::NORMAL) - px(8.)).max(px(0.));

    div()
        .id(SharedString::from(format!(
            "recent-tile-{}",
            item.server_id
        )))
        .min_w_0()
        .h(px(58.))
        .pl(RECENT_PADDING_LEFT)
        .pr(RECENT_PADDING_RIGHT)
        .rounded(px(11.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .when(!p.is_dark, |tile| tile.shadow_xs())
        .flex()
        .items_center()
        .gap(RECENT_GAP)
        .cursor_pointer()
        .hover(move |s| s.border_color(hover_border))
        .on_click(move |_, _, cx| {
            connect_to_server(&server_id, &server_name, &session_state, cx);
        })
        .child(render_icon_tile(icon, icon_color, RECENT_ICON_TILE, 18.))
        .child(
            div()
                .flex_none()
                .w(text_width)
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            ctx.fit
                                .label(&item.name, name_width, 13., FontWeight::MEDIUM)
                                .text_color(p.text),
                        )
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_size(px(TIME_SIZE))
                                .text_color(p.text_faint)
                                .child(time),
                        ),
                )
                .child(
                    ctx.fit
                        .mono_label(&item.address, text_width, 11.)
                        .text_color(p.text_faint),
                ),
        )
}

/// 渲染服务器操作菜单所在的浮层。
///
/// 外层铺满窗口并拦截鼠标，菜单开着时点别处只会关掉菜单，不会点到下面的内容。
fn render_menu_layer(
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    anchor: Anchor,
    viewport: Size<Pixels>,
) -> impl IntoElement {
    deferred(
        anchored().position(Point::default()).child(
            div().w(viewport.width).h(viewport.height).occlude().child(
                anchored()
                    .position(position)
                    .anchor(anchor)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            ),
        ),
    )
    .with_priority(1)
}

/// 删除确认对话框；没有待确认的删除时返回 `None`
pub fn render_delete_confirm(
    view_state: &Entity<ViewModeState>,
    dialog_state: &Entity<ServerDialogState>,
    lang: &Language,
    cx: &App,
) -> Option<impl IntoElement> {
    let pending = view_state.read(cx).pending_delete.as_ref()?;
    let server_id = pending.server_id.clone();
    let text = ConfirmDialogText {
        title: i18n::t(lang, "server_list.delete_title").into(),
        message: i18n::t(lang, "server_list.delete_message")
            .replace("{}", &pending.server_name)
            .into(),
        confirm: i18n::t(lang, "common.delete").into(),
        cancel: i18n::t(lang, "common.cancel").into(),
    };

    let close = {
        let view_state = view_state.clone();
        move |cx: &mut App| {
            view_state.update(cx, |state, cx| {
                state.pending_delete = None;
                cx.notify();
            });
        }
    };
    let on_cancel = {
        let close = close.clone();
        move |_: &mut Window, cx: &mut App| close(cx)
    };
    let on_confirm = {
        let dialog_state = dialog_state.clone();
        move |_: &mut Window, cx: &mut App| {
            if let Err(e) = storage::delete_server(&server_id) {
                error!("Failed to delete server: {}", e);
            }
            dialog_state.update(cx, |state, _| {
                state.needs_refresh = true;
            });
            close(cx);
        }
    };

    Some(render_confirm_dialog(
        text,
        &pending.focus_handle,
        on_confirm,
        on_cancel,
        cx,
    ))
}

fn copy_endpoint_with_feedback(
    endpoint: String,
    server_id: String,
    view_state: Entity<ViewModeState>,
    cx: &mut App,
) {
    cx.write_to_clipboard(ClipboardItem::new_string(endpoint));

    let token = view_state.read(cx).copy_feedback_token.wrapping_add(1);
    view_state.update(cx, |state, cx| {
        state.copied_server_id = Some(server_id.clone());
        state.copy_feedback_token = token;
        cx.notify();
    });

    cx.spawn(async move |async_cx| {
        async_cx
            .background_executor()
            .timer(std::time::Duration::from_millis(1200))
            .await;

        let _ = async_cx.update(|cx| {
            view_state.update(cx, |state, cx| {
                if state.copy_feedback_token == token
                    && state.copied_server_id.as_deref() == Some(server_id.as_str())
                {
                    state.copied_server_id = None;
                    cx.notify();
                }
            });
        });
    })
    .detach();
}

/// 优先使用连接后采集到的 OS；未采集时从名称和描述中做保守识别。
fn distribution_icon(server: &Server) -> Option<&'static str> {
    detect_distribution_icon(server.os_name.as_deref(), &server.name, &server.description)
}

/// 空状态的外框：居中的图标和标题，调用方在下面追加说明和按钮
fn empty_state(icon: &'static str, title: &'static str, p: &Palette) -> Div {
    div()
        .flex_1()
        .min_h(px(0.))
        .pb(px(48.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .size(px(56.))
                .mb_1()
                .rounded(px(14.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .flex()
                .items_center()
                .justify_center()
                .child(icon_sized(icon, 22., p.text_muted)),
        )
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title),
        )
}

/// 渲染搜索无结果的状态
fn render_search_empty(query: &str, ctx: &Ctx) -> impl IntoElement {
    let p = ctx.p;
    empty_state(icons::SEARCH, ctx.t("server_list.no_results_title"), &p)
        .children((!query.is_empty()).then(|| {
            div()
                .px_2()
                .py(px(2.))
                .rounded(px(6.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(
                    ctx.fit
                        .mono_label(query, px(320.), 12.)
                        .text_color(p.text_soft),
                )
        }))
        .child(
            div()
                .text_color(p.text_muted)
                .child(ctx.t("server_list.no_results_description")),
        )
}

/// 渲染空状态（没有服务器时显示）
fn render_empty_state(ctx: &Ctx) -> impl IntoElement {
    let p = ctx.p;
    let dialog_state = ctx.dialog_state.clone();
    empty_state(icons::SERVER, ctx.t("server_list.empty_title"), &p)
        .child(
            div()
                .text_color(p.text_muted)
                .child(ctx.t("server_list.empty_description")),
        )
        .child(
            div().mt_2().child(
                primary_button(
                    "empty-add-server-btn",
                    icons::PLUS,
                    ctx.t("server_list.add_server"),
                    &p,
                )
                .on_click(move |_, _, cx| {
                    dialog_state.update(cx, |s, _| s.open_add());
                }),
            ),
        )
}

/// 渲染占位内容
pub fn render_placeholder(title: &str, description: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_4()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_base()
                .text_color(cx.theme().muted_foreground)
                .child(description.to_string()),
        )
        .child(
            div()
                .mt_4()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Coming soon..."),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        distribution_icon, filter_server_groups, visible_groups, Metrics, RowColumns, ViewMode,
        ViewModeState,
    };
    use crate::constants::icons;
    use crate::models::{Server, ServerGroup};
    use gpui_kit::px;

    fn server(name: &str, host: &str, account: &str, port: u16) -> Server {
        Server {
            id: name.to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port,
            description: format!("{} node", name),
            account: account.to_string(),
            os_name: None,
            last_connected_at: None,
        }
    }

    fn groups() -> Vec<ServerGroup> {
        vec![
            ServerGroup {
                id: "g-prod".to_string(),
                name: "Production".to_string(),
                servers: vec![server("API", "10.0.0.10", "root", 22)],
            },
            ServerGroup {
                id: "g-staging".to_string(),
                name: "Staging".to_string(),
                servers: vec![server("Worker", "staging.internal", "deploy", 2202)],
            },
        ]
    }

    #[test]
    fn empty_query_keeps_all_servers() {
        let filtered = filter_server_groups(&groups(), "  ");
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered.iter().map(|g| g.servers.len()).sum::<usize>(), 2);
    }

    #[test]
    fn search_matches_server_fields_case_insensitively() {
        let filtered = filter_server_groups(&groups(), "STAGING.INTERNAL");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].servers[0].name, "Worker");

        let filtered = filter_server_groups(&groups(), "2202");
        assert_eq!(filtered[0].servers[0].account, "deploy");
    }

    #[test]
    fn matching_group_name_keeps_the_whole_group() {
        let filtered = filter_server_groups(&groups(), "production");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Production");
        assert_eq!(filtered[0].id, "g-prod");
        assert_eq!(filtered[0].servers.len(), 1);
    }

    #[test]
    fn group_filter_narrows_before_search() {
        let visible = visible_groups(&groups(), Some("g-staging"), "");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "Staging");

        // 搜索词只在选中的分组里匹配
        assert!(visible_groups(&groups(), Some("g-staging"), "10.0.0.10").is_empty());
        assert_eq!(visible_groups(&groups(), None, "10.0.0.10").len(), 1);
    }

    #[test]
    fn group_filter_only_applies_in_card_view() {
        let mut state = ViewModeState::default();
        state.toggle_group_filter("g-prod");
        assert_eq!(state.active_group_filter(), None);

        state.mode = ViewMode::Card;
        assert_eq!(state.active_group_filter(), Some("g-prod"));

        // 再点一次取消
        state.toggle_group_filter("g-prod");
        assert_eq!(state.active_group_filter(), None);
    }

    #[test]
    fn removed_groups_lose_their_filter_and_collapse_state() {
        let mut state = ViewModeState {
            mode: ViewMode::Card,
            ..Default::default()
        };
        state.toggle_group_filter("g-staging");
        state.toggle_group_collapsed("g-staging");
        state.toggle_group_collapsed("g-prod");

        let remaining = &groups()[..1];
        state.retain_groups(remaining);

        assert_eq!(state.active_group_filter(), None);
        assert!(state.collapsed_groups.contains("g-prod"));
        assert!(!state.collapsed_groups.contains("g-staging"));
    }

    #[test]
    fn collapsing_a_group_twice_expands_it_again() {
        let mut state = ViewModeState::default();
        state.toggle_group_collapsed("g-prod");
        assert!(state.collapsed_groups.contains("g-prod"));
        state.toggle_group_collapsed("g-prod");
        assert!(state.collapsed_groups.is_empty());
    }

    /// 一行里各列加上固定部分和间距后的总宽度
    fn row_total(columns: RowColumns) -> gpui_kit::Pixels {
        // 左右留白 12 + 8，图标 20，时间 88，连接按钮 28
        let fixed = px(12. + 8. + 20. + 88. + 28.);
        let gaps = if columns.description.is_some() {
            5.
        } else {
            4.
        };
        fixed
            + columns.name
            + columns.description.unwrap_or(px(0.))
            + columns.address
            + px(12.) * gaps
    }

    #[test]
    fn row_columns_at_the_default_window_size() {
        // 1200 宽的窗口：内容区 980，行宽 956
        let columns = RowColumns::new(px(956.), false);
        assert_eq!(
            columns,
            RowColumns {
                name: px(180.),
                description: Some(px(194.)),
                address: px(366.),
            }
        );
        assert_eq!(row_total(columns), px(956.));
    }

    #[test]
    fn wide_rows_cap_the_address_and_give_the_rest_to_the_description() {
        // 1600 宽的窗口
        let columns = RowColumns::new(px(1356.), false);
        assert_eq!(columns.address, px(440.));
        assert_eq!(columns.description, Some(px(520.)));
        assert_eq!(row_total(columns), px(1356.));
    }

    #[test]
    fn narrow_rows_shrink_description_and_address_in_proportion() {
        let columns = RowColumns::new(px(756.), false);
        assert_eq!(columns.description, Some(px(120.)));
        assert_eq!(columns.address, px(240.));
        assert_eq!(row_total(columns), px(756.));
    }

    #[test]
    fn compact_rows_drop_the_description() {
        // 900 宽的窗口
        let columns = RowColumns::new(px(656.), true);
        assert_eq!(columns.description, None);
        assert_eq!(columns.address, px(250.));
        assert_eq!(columns.name, px(202.));
        assert_eq!(row_total(columns), px(656.));

        // 更窄时名称至少保留四成
        let columns = RowColumns::new(px(456.), true);
        assert_eq!(columns.name, px(100.));
        assert_eq!(columns.address, px(152.));
        assert_eq!(row_total(columns), px(456.));
    }

    #[test]
    fn row_columns_never_go_negative() {
        for compact in [false, true] {
            let columns = RowColumns::new(px(40.), compact);
            assert!(columns.name >= px(0.));
            assert!(columns.address >= px(0.));
            assert!(columns.description.unwrap_or(px(0.)) >= px(0.));
        }
    }

    #[test]
    fn metrics_follow_the_content_width() {
        // 1200 宽的窗口：三列卡片、四个「最近连接」
        let metrics = Metrics::new(px(980.));
        assert!(metrics.row.description.is_some());
        assert_eq!(metrics.row_width, px(956.));
        assert_eq!(metrics.card_columns, 3);
        assert_eq!(metrics.recent_columns, 4);
        // 卡片宽 (932 - 24) / 3 ≈ 302.67，去掉边框和内边距 30
        assert_eq!(metrics.card_inner_width, px(272.));
        // 条目宽 (932 - 36) / 4 = 224，去掉边框、留白、图标块和间距 66
        assert_eq!(metrics.recent_text_width, px(158.));

        // 900 宽的窗口：两列卡片，列表不显示描述列
        let metrics = Metrics::new(px(680.));
        assert!(metrics.row.description.is_none());
        assert_eq!(metrics.card_columns, 2);
        assert_eq!(metrics.recent_columns, 3);

        // 1600 宽的窗口
        let metrics = Metrics::new(px(1380.));
        assert_eq!(metrics.card_columns, 4);
        assert_eq!(metrics.recent_columns, 6);
    }

    #[test]
    fn distribution_prefers_collected_os_name() {
        let mut host = server("Production", "10.0.0.10", "root", 22);
        host.os_name = Some("Ubuntu 24.04.1 LTS".to_string());
        assert_eq!(distribution_icon(&host), Some(icons::DISTRO_UBUNTU));
    }

    #[test]
    fn distribution_falls_back_to_profile_text() {
        let host = server("Debian backup", "10.0.0.11", "root", 22);
        assert_eq!(distribution_icon(&host), Some(icons::DISTRO_DEBIAN));

        let unknown = server("Database", "10.0.0.12", "root", 22);
        assert_eq!(distribution_icon(&unknown), None);
    }
}
