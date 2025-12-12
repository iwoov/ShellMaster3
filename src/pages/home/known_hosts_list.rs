// Known Hosts 列表页面组件

use gpui::*;
use gpui_component::ActiveTheme;
use tracing::error;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::KnownHost;
use crate::services::storage;

/// Known Hosts 页面状态
pub struct KnownHostsPageState {
    /// 已知主机列表
    pub hosts: Vec<KnownHost>,
    /// 刷新标记
    pub needs_refresh: bool,
}

impl KnownHostsPageState {
    pub fn new() -> Self {
        let hosts = storage::load_known_hosts()
            .map(|c| c.hosts)
            .unwrap_or_default();
        Self {
            hosts,
            needs_refresh: false,
        }
    }

    /// 刷新列表
    pub fn refresh(&mut self) {
        self.hosts = storage::load_known_hosts()
            .map(|c| c.hosts)
            .unwrap_or_default();
        self.needs_refresh = false;
    }

    /// 删除主机
    pub fn delete_host(&mut self, host_key: &str) {
        // host_key 格式为 "host:port"
        if let Some(pos) = host_key.rfind(':') {
            let host = &host_key[..pos];
            if let Ok(port) = host_key[pos + 1..].parse::<u16>() {
                if let Err(e) = storage::remove_known_host(host, port) {
                    error!("Failed to remove known host: {}", e);
                }
            }
        }
        self.refresh();
    }
}

impl Default for KnownHostsPageState {
    fn default() -> Self {
        Self::new()
    }
}

/// 卡片颜色配置
#[derive(Clone, Copy)]
struct CardColors {
    bg: Hsla,
    border: Hsla,
    primary: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    destructive: Hsla,
}

/// 渲染 Known Hosts 内容区域
pub fn render_known_hosts_content(
    state: Entity<KnownHostsPageState>,
    cx: &App,
) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let colors = CardColors {
        bg: cx.theme().popover,
        border: cx.theme().border,
        primary: cx.theme().primary,
        foreground: cx.theme().foreground,
        muted_foreground: cx.theme().muted_foreground,
        destructive: rgb(0xef4444).into(),
    };

    let hosts: Vec<KnownHost> = state.read(cx).hosts.clone();
    let has_items = !hosts.is_empty();

    let bg_color = crate::theme::background_color(cx);

    div()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .bg(bg_color)
        .flex()
        .flex_col()
        .relative()
        // 标题区域
        .child(render_header(&lang, colors, hosts.len()))
        // 卡片内容区域
        .child(if has_items {
            div()
                .id("known-hosts-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px_6()
                .pb_6()
                .child(render_card_grid(state, hosts, colors))
                .into_any_element()
        } else {
            render_empty_state(&lang, colors).into_any_element()
        })
}

/// 渲染头部区域
fn render_header(lang: &Language, colors: CardColors, count: usize) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .p_6()
        .pb_4()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(colors.muted_foreground)
                .child(format!("{} {}", count, i18n::t(lang, "known_hosts.items"))),
        )
}

/// 渲染卡片网格
fn render_card_grid(
    state: Entity<KnownHostsPageState>,
    hosts: Vec<KnownHost>,
    colors: CardColors,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_4()
        .children(hosts.into_iter().map(|host| {
            let state_clone = state.clone();
            render_host_card(state_clone, host, colors)
        }))
}

/// 渲染单个主机卡片
fn render_host_card(
    state: Entity<KnownHostsPageState>,
    host: KnownHost,
    colors: CardColors,
) -> impl IntoElement {
    let host_key = host.host.clone();
    let host_key_for_delete = host_key.clone();

    div()
        .id(SharedString::from(format!("known-host-{}", host_key)))
        .w(px(280.))
        .bg(colors.bg)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .p_4()
        .hover(move |s| s.border_color(colors.primary.opacity(0.5)).shadow_md())
        .flex()
        .flex_col()
        .gap_3()
        // 顶部：图标和主机地址
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w_10()
                                .h_10()
                                .rounded_lg()
                                .bg(colors.primary.opacity(0.1))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(render_icon(icons::FINGERPRINT, colors.primary.into())),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground)
                                        .child(host.host.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(colors.muted_foreground)
                                        .child(host.key_type.clone()),
                                ),
                        ),
                )
                // 删除按钮
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "delete-known-host-{}",
                            host_key
                        )))
                        .cursor_pointer()
                        .p(px(6.0))
                        .rounded_md()
                        .hover(move |s| s.bg(colors.destructive.opacity(0.1)))
                        .on_click(move |_, _, cx| {
                            state.update(cx, |s, _| {
                                s.delete_host(&host_key_for_delete);
                            });
                        })
                        .child(render_icon(icons::TRASH, colors.destructive.into())),
                ),
        )
        // 指纹信息
        .child(
            div()
                .px_2()
                .py_1()
                .bg(colors.muted_foreground.opacity(0.05))
                .rounded_md()
                .child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(colors.muted_foreground)
                        .overflow_hidden()
                        .child(truncate_fingerprint(&host.fingerprint, 40)),
                ),
        )
        // 底部：时间信息
        .child(
            div()
                .flex()
                .justify_between()
                .text_xs()
                .text_color(colors.muted_foreground)
                .child(format!("首次: {}", format_date(&host.first_seen)))
                .child(format!("最近: {}", format_date(&host.last_used))),
        )
}

/// 渲染空状态
fn render_empty_state(lang: &Language, colors: CardColors) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .child(
            div()
                .w_16()
                .h_16()
                .rounded_full()
                .bg(colors.primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::FINGERPRINT, colors.primary.into())),
        )
        .child(
            div()
                .text_lg()
                .text_color(colors.foreground)
                .child(i18n::t(lang, "known_hosts.empty.title")),
        )
        .child(
            div()
                .text_sm()
                .text_color(colors.muted_foreground)
                .text_center()
                .max_w(px(300.))
                .child(i18n::t(lang, "known_hosts.empty.description")),
        )
}

/// 截断指纹显示
fn truncate_fingerprint(fp: &str, max_len: usize) -> String {
    if fp.len() <= max_len {
        fp.to_string()
    } else {
        format!("{}...", &fp[..max_len])
    }
}

/// 格式化日期显示（只显示日期部分）
fn format_date(datetime: &str) -> String {
    // 输入格式: "2024-12-11 15:30:00"
    // 输出格式: "2024-12-11"
    datetime.split(' ').next().unwrap_or(datetime).to_string()
}
