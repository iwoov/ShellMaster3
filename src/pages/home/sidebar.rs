// 主页左侧菜单栏

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::server_list::{connect_to_server, ViewModeState};
use super::titlebar::render_sidebar_brand;
use super::widgets::{TextFitter, SIDEBAR_WIDTH};
use crate::components::common::icon::{icon_sized, server_icon};
use crate::components::common::settings_dialog::SettingsDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::HistoryItem;
use crate::state::SessionState;
use crate::theme::Palette;

/// 侧边栏条目内部用于联动悬停样式的分组名
const ITEM_GROUP: &str = "sidebar-item";
/// 侧边栏内容和条目自身的左右留白
const SECTION_PADDING: Pixels = px(8.);
const ITEM_PADDING: Pixels = px(8.);
/// 条目内图标、文字、右侧小字之间的间距
const ITEM_GAP: Pixels = px(9.);
const ITEM_ICON_SIZE: f32 = 16.;
/// 条目右侧小字（数量、时间、版本号）的字号
const ITEM_META_SIZE: f32 = 11.;

/// 条目里图标右边可用的宽度：侧边栏宽度减去右边框、两层左右留白、图标和它后面的间距
fn item_text_width() -> Pixels {
    SIDEBAR_WIDTH
        - px(1.)
        - SECTION_PADDING * 2.
        - ITEM_PADDING * 2.
        - px(ITEM_ICON_SIZE)
        - ITEM_GAP
}
/// 「最近连接」最多显示的条数
const RECENT_LIMIT: usize = 6;

/// 菜单类型
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuType {
    Hosts,
    /// 主机监控页还只是占位，入口先不放进导航（见 [`NAV_ITEMS`]）
    #[allow(dead_code)]
    Monitor,
    Snippets,
    KnownHosts,
}

/// 导航里显示的页面，按显示顺序排列
const NAV_ITEMS: [MenuType; 3] = [MenuType::Hosts, MenuType::Snippets, MenuType::KnownHosts];

impl MenuType {
    pub fn id(&self) -> &'static str {
        match self {
            MenuType::Hosts => "hosts",
            MenuType::Monitor => "monitor",
            MenuType::Snippets => "snippets",
            MenuType::KnownHosts => "known_hosts",
        }
    }

    pub fn label_key(&self) -> &'static str {
        match self {
            MenuType::Hosts => "sidebar.hosts",
            MenuType::Monitor => "sidebar.monitor",
            MenuType::Snippets => "sidebar.snippets",
            MenuType::KnownHosts => "sidebar.known_hosts",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            MenuType::Hosts => icons::SERVER,
            MenuType::Monitor => icons::MONITOR,
            MenuType::Snippets => icons::CODE,
            MenuType::KnownHosts => icons::FINGERPRINT,
        }
    }
}

/// 侧边栏状态
pub struct SidebarState {
    pub selected_menu: MenuType,
}

/// 导航项右侧显示的数量
#[derive(Clone, Copy, Default)]
pub struct NavCounts {
    pub servers: usize,
    pub snippets: usize,
    pub known_hosts: usize,
}

impl NavCounts {
    fn for_menu(&self, menu: MenuType) -> Option<usize> {
        match menu {
            MenuType::Hosts => Some(self.servers),
            MenuType::Snippets => Some(self.snippets),
            MenuType::KnownHosts => Some(self.known_hosts),
            MenuType::Monitor => None,
        }
    }
}

/// 侧边栏里的一个服务器分组
pub struct GroupEntry {
    pub id: String,
    pub name: String,
    pub server_count: usize,
}

/// 导航下方显示的内容
pub enum SidebarSection<'a> {
    /// 最近连接过的服务器，点击直接连接
    Recent(&'a [HistoryItem]),
    /// 服务器分组，点击后卡片视图只显示该组（卡片视图把「最近连接」放到了内容区顶部）
    Groups {
        groups: Vec<GroupEntry>,
        selected: Option<String>,
    },
}

/// 渲染侧边栏所需的数据
pub struct SidebarProps<'a> {
    pub fit: TextFitter,
    pub state: Entity<SidebarState>,
    pub selected_menu: MenuType,
    pub counts: NavCounts,
    pub section: SidebarSection<'a>,
    pub view_state: Entity<ViewModeState>,
    pub settings_dialog_state: Entity<SettingsDialogState>,
    pub session_state: Entity<SessionState>,
}

/// 渲染侧边栏（从窗口顶部到底部）
pub fn render_sidebar(props: SidebarProps, cx: &App) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let fit = props.fit;
    let lang = props
        .settings_dialog_state
        .read(cx)
        .settings
        .theme
        .language
        .clone();
    let settings_dialog_state = props.settings_dialog_state.clone();

    let (section_title, section_items) = match props.section {
        SidebarSection::Recent(history) => (
            i18n::t(&lang, "sidebar.recent"),
            render_recent_items(history, &lang, &props.session_state, &fit, &p),
        ),
        SidebarSection::Groups { groups, selected } => (
            i18n::t(&lang, "sidebar.groups"),
            render_group_items(groups, selected, &props.view_state, &fit, &p),
        ),
    };

    div()
        .w(SIDEBAR_WIDTH)
        .h_full()
        .flex_none()
        .bg(p.sidebar)
        .border_r_1()
        .border_color(p.border)
        .text_size(px(13.))
        .flex()
        .flex_col()
        .child(render_sidebar_brand(cx))
        // 页面导航
        .child(
            div()
                .flex_none()
                .px(SECTION_PADDING)
                .pt_2()
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(NAV_ITEMS.into_iter().map(|menu| {
                    let selected = props.selected_menu == menu;
                    let state = props.state.clone();
                    let view_state = props.view_state.clone();
                    sidebar_item(menu.id(), selected, &p)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |state, cx| {
                                state.selected_menu = menu;
                                cx.notify();
                            });
                            // 「服务器」同时是「全部服务器」的入口：点它会清掉分组筛选
                            if menu == MenuType::Hosts {
                                view_state.update(cx, |state, cx| {
                                    state.group_filter = None;
                                    cx.notify();
                                });
                            }
                        })
                        .child(item_icon(
                            menu.icon(),
                            if selected { p.primary } else { p.text_muted },
                            selected,
                            &p,
                        ))
                        .children(item_text(
                            i18n::t(&lang, menu.label_key()),
                            props.counts.for_menu(menu).map(|count| count.to_string()),
                            selected,
                            &fit,
                            &p,
                        ))
                })),
        )
        // 最近连接或分组
        .child(
            div()
                .flex_none()
                .px_4()
                .pt_5()
                .pb(px(6.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text_faint)
                .child(section_title),
        )
        .child(
            div()
                .id("sidebar-section-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px(SECTION_PADDING)
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(section_items),
        )
        // 设置
        .child(
            div().flex_none().p(SECTION_PADDING).child(
                sidebar_item("settings-btn", false, &p)
                    .on_click(move |_, _, cx| {
                        settings_dialog_state.update(cx, |s, _| s.open());
                    })
                    .child(item_icon(icons::SETTINGS, p.text_muted, false, &p))
                    .children(item_text(
                        i18n::t(&lang, "sidebar.settings"),
                        Some(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        false,
                        &fit,
                        &p,
                    )),
            ),
        )
}

/// 侧边栏条目的外框：30px 高的一行，悬停和选中时有底色
fn sidebar_item(id: impl Into<ElementId>, selected: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, hover_text) = (p.hover, p.text);
    div()
        .id(id)
        .group(ITEM_GROUP)
        .flex_none()
        .h(px(30.))
        .px(ITEM_PADDING)
        .rounded(px(6.))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(ITEM_GAP)
        .text_color(p.text_muted)
        .when(selected, |item| item.bg(p.selected).text_color(p.text))
        .when(!selected, |item| {
            item.hover(move |s| s.bg(hover_bg).text_color(hover_text))
        })
}

/// 条目左侧的图标；未选中的条目悬停时图标随文字一起提亮
fn item_icon(icon: &'static str, color: Hsla, selected: bool, p: &Palette) -> impl IntoElement {
    let hover_color = p.text;
    icon_sized(icon, ITEM_ICON_SIZE, color).when(!selected, |icon| {
        icon.group_hover(ITEM_GROUP, move |s| s.text_color(hover_color))
    })
}

/// 条目的文字，以及右侧的弱色小字（数量、时间、版本号）。
/// 文字占用小字之外的全部宽度，放不下时末尾显示省略号。
fn item_text(
    label: &str,
    meta: Option<String>,
    selected: bool,
    fit: &TextFitter,
    p: &Palette,
) -> [Div; 2] {
    let weight = if selected {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    };
    let meta_width = meta.as_deref().map_or(px(0.), |meta| {
        fit.width(meta, ITEM_META_SIZE, FontWeight::NORMAL) + ITEM_GAP
    });
    let label_width = (item_text_width() - meta_width).max(px(0.));

    [
        fit.label(label, label_width, 13., weight).flex_1(),
        div()
            .flex_none()
            .whitespace_nowrap()
            .text_size(px(ITEM_META_SIZE))
            .text_color(p.text_faint)
            .children(meta),
    ]
}

/// 「最近连接」的条目，点击直接连接
fn render_recent_items(
    history: &[HistoryItem],
    lang: &Language,
    session_state: &Entity<SessionState>,
    fit: &TextFitter,
    p: &Palette,
) -> Vec<AnyElement> {
    if history.is_empty() {
        return vec![div()
            .px(ITEM_PADDING)
            .py_1()
            .text_size(px(12.))
            .text_color(p.text_faint)
            .child(i18n::t(lang, "sidebar.no_history"))
            .into_any_element()];
    }

    let now = chrono::Local::now().naive_local();
    history
        .iter()
        .take(RECENT_LIMIT)
        .map(|item| {
            let (icon, icon_color) = server_icon(item.distro_icon, p.is_dark, p.text_faint);
            let session_state = session_state.clone();
            let (server_id, server_name) = (item.server_id.clone(), item.name.clone());

            sidebar_item(
                SharedString::from(format!("recent-server-{}", item.server_id)),
                false,
                p,
            )
            .on_click(move |_, _, cx| {
                connect_to_server(&server_id, &server_name, &session_state, cx);
            })
            // 发行版图标保持品牌色，不随悬停变化；实心的品牌图标比线条图标显大，缩小 1px
            .child(
                div()
                    .flex_none()
                    .size(px(ITEM_ICON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_sized(icon, ITEM_ICON_SIZE - 1., icon_color)),
            )
            .children(item_text(
                &item.name,
                Some(i18n::relative_time(lang, item.connected_at, now)),
                false,
                fit,
                p,
            ))
            .into_any_element()
        })
        .collect()
}

/// 分组条目，点击后卡片视图只显示该组，再点一次取消
fn render_group_items(
    groups: Vec<GroupEntry>,
    selected: Option<String>,
    view_state: &Entity<ViewModeState>,
    fit: &TextFitter,
    p: &Palette,
) -> Vec<AnyElement> {
    groups
        .into_iter()
        .map(|group| {
            let is_selected = selected.as_deref() == Some(group.id.as_str());
            let view_state = view_state.clone();
            let group_id = group.id.clone();

            sidebar_item(
                SharedString::from(format!("sidebar-group-{}", group.id)),
                is_selected,
                p,
            )
            .on_click(move |_, _, cx| {
                view_state.update(cx, |state, cx| {
                    state.toggle_group_filter(&group_id);
                    cx.notify();
                });
            })
            .child(item_icon(
                if is_selected {
                    icons::FOLDER_OPEN
                } else {
                    icons::FOLDER
                },
                if is_selected { p.primary } else { p.text_muted },
                is_selected,
                p,
            ))
            .children(item_text(
                &group.name,
                Some(group.server_count.to_string()),
                is_selected,
                fit,
                p,
            ))
            .into_any_element()
        })
        .collect()
}
