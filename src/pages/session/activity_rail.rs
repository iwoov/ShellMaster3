// 会话视图最右边的图标栏：切换右侧侧栏的面板（快捷命令 / 传输 / AI），以及收起、展开侧栏

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::columns::RAIL_WIDTH;
use crate::components::common::button::{icon_button, icon_toggle_button};
use crate::components::common::panel::status_dot;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::state::{SessionState, SidebarPanel};
use crate::theme::Palette;

/// 图标栏里的三个面板，按显示顺序排列
const PANELS: [(SidebarPanel, &str, &str); 3] = [
    (
        SidebarPanel::Snippets,
        icons::COMMAND,
        "session.rail.snippets",
    ),
    (
        SidebarPanel::Transfer,
        icons::TRANSFER,
        "session.rail.transfers",
    ),
    (SidebarPanel::AiChat, icons::SPARKLES, "session.rail.ai"),
];

/// 渲染图标栏。`transfers_running` 为 true 时传输按钮右上角显示小圆点
pub fn render_activity_rail(
    session_state: &Entity<SessionState>,
    transfers_running: bool,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let state = session_state.read(cx);
    let collapsed = state.sidebar_collapsed;
    let active_panel = state.active_sidebar_panel;

    let toggle = {
        let session_state = session_state.clone();
        icon_button(
            "session-rail-toggle",
            if collapsed {
                icons::PANEL_RIGHT_OPEN
            } else {
                icons::PANEL_RIGHT_CLOSE
            },
            i18n::t(
                lang,
                if collapsed {
                    "session.rail.expand"
                } else {
                    "session.rail.collapse"
                },
            ),
            p,
        )
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.toggle_sidebar();
                cx.notify();
            });
        })
    };

    div()
        .flex_none()
        .w(RAIL_WIDTH)
        .h_full()
        .bg(p.sidebar)
        .border_l_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .items_center()
        // 第一个按钮与侧栏 36px 标题条垂直居中
        .pt(px(4.))
        .pb(px(8.))
        .gap(px(4.))
        .children(PANELS.into_iter().map(|(panel, icon, label)| {
            // 侧栏收起时不显示选中态，避免看起来像是面板还开着
            let active = !collapsed && active_panel == panel;
            let session_state = session_state.clone();
            let button = icon_toggle_button(
                SharedString::from(format!("session-rail-{}", label)),
                icon,
                active,
                i18n::t(lang, label),
                p,
            )
            .on_click(move |_, _, cx| {
                session_state.update(cx, |state, cx| {
                    if !state.sidebar_collapsed && state.active_sidebar_panel == panel {
                        // 再点一次当前面板：收起侧栏
                        state.toggle_sidebar();
                    } else {
                        if panel == SidebarPanel::Snippets {
                            state.load_snippets_config();
                        }
                        state.set_sidebar_panel(panel);
                    }
                    cx.notify();
                });
            });

            let show_dot = panel == SidebarPanel::Transfer && transfers_running;
            div().relative().child(button).when(show_dot, |wrapper| {
                wrapper.child(
                    div()
                        .absolute()
                        .top(px(4.))
                        .right(px(4.))
                        .child(status_dot(p.primary, 6.).border_1().border_color(p.sidebar)),
                )
            })
        }))
        .child(div().flex_1())
        .child(toggle)
}
