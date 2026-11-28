// SFTP 表头：目录树一栏（标题 + 主目录 / 刷新 / 显示隐藏文件）
// 和文件列表一栏（后退 / 前进 / 上级 + 路径框 + 过滤 / 新建文件夹 / 上传下拉 / 下载）

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::component::Sizable;
use gpui_kit::*;

use super::file_list::menu_item_element;
use super::PathBarState;
use crate::components::common::button::{
    icon_button_enabled, icon_toggle_button, ICON_BUTTON_SIZE,
};
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{panel_header, PANEL_PADDING};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::SftpState;
use crate::theme::Palette;

/// SFTP 工具栏事件
#[derive(Clone, Debug)]
pub enum SftpToolbarEvent {
    GoBack,
    GoForward,
    GoUp,
    GoHome,
    Refresh,
    NewFolder,
    ToggleHidden,
    ToggleFilter,
    UploadFile,
    UploadFolder,
    Download,
}

/// 一个工具栏按钮：可用时绑定点击，不可用时变淡
#[allow(clippy::too_many_arguments)]
fn toolbar_button<F>(
    id: &'static str,
    icon: &'static str,
    tooltip: &'static str,
    enabled: bool,
    event: SftpToolbarEvent,
    on_event: &F,
    lang: &Language,
    p: &Palette,
) -> Stateful<Div>
where
    F: Fn(SftpToolbarEvent, &mut Window, &mut App) + Clone + 'static,
{
    let button = icon_button_enabled(id, icon, i18n::t(lang, tooltip), enabled, p);
    if enabled {
        let on_event = on_event.clone();
        button.on_click(move |_, window, cx| on_event(event.clone(), window, cx))
    } else {
        button
    }
}

/// 目录树一栏的表头
pub fn render_sftp_tree_header<F>(
    state: Option<&SftpState>,
    on_event: F,
    lang: &Language,
    p: &Palette,
) -> impl IntoElement
where
    F: Fn(SftpToolbarEvent, &mut Window, &mut App) + Clone + 'static,
{
    let connected = state.is_some();
    let show_hidden = state.map(|s| s.show_hidden).unwrap_or(false);

    let hidden_toggle = {
        let toggle = icon_toggle_button(
            "sftp-btn-toggle-hidden",
            icons::EYE,
            show_hidden,
            i18n::t(lang, "sftp.toolbar.show_hidden"),
            p,
        );
        if connected {
            let on_event = on_event.clone();
            toggle.on_click(move |_, window, cx| {
                on_event(SftpToolbarEvent::ToggleHidden, window, cx)
            })
        } else {
            toggle.opacity(0.4)
        }
    };

    panel_header(p)
        .bg(p.sidebar)
        .pl(PANEL_PADDING)
        .pr(px(6.))
        .gap(px(2.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text_muted)
                .child(i18n::t(lang, "sftp.panel.title")),
        )
        .child(toolbar_button(
            "sftp-btn-home",
            icons::HOME,
            "sftp.toolbar.home",
            connected,
            SftpToolbarEvent::GoHome,
            &on_event,
            lang,
            p,
        ))
        .child(toolbar_button(
            "sftp-btn-refresh",
            icons::REFRESH,
            "sftp.toolbar.refresh",
            connected,
            SftpToolbarEvent::Refresh,
            &on_event,
            lang,
            p,
        ))
        .child(hidden_toggle)
}

/// 文件列表一栏的表头
#[allow(clippy::too_many_arguments)]
pub fn render_sftp_list_header<F>(
    state: Option<&SftpState>,
    path_bar_state: Option<Entity<PathBarState>>,
    has_selection: bool,
    filter_open: bool,
    on_event: F,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement
where
    F: Fn(SftpToolbarEvent, &mut Window, &mut App) + Clone + 'static,
{
    let connected = state.is_some();
    let (can_back, can_forward, can_up) = match state {
        Some(s) => (s.can_go_back(), s.can_go_forward(), s.can_go_up()),
        None => (false, false, false),
    };

    let filter_toggle = {
        let toggle = icon_toggle_button(
            "sftp-btn-filter",
            icons::SEARCH,
            filter_open,
            i18n::t(lang, "sftp.toolbar.filter"),
            p,
        );
        if connected {
            let on_event = on_event.clone();
            toggle.on_click(move |_, window, cx| {
                on_event(SftpToolbarEvent::ToggleFilter, window, cx)
            })
        } else {
            toggle.opacity(0.4)
        }
    };

    // 上传下拉按钮：支持分别选择「上传文件」或「上传文件夹」
    let upload_control: AnyElement = if connected {
        let style = ButtonCustomVariant::new(cx)
            .color(gpui_kit::transparent_black())
            .foreground(p.text_muted)
            .hover(p.hover_strong)
            .active(p.hover_strong);
        let upload_file_label = i18n::t(lang, "sftp.context_menu.upload_file").to_string();
        let upload_folder_label = i18n::t(lang, "sftp.context_menu.upload_folder").to_string();
        let on_file = on_event.clone();
        let on_folder = on_event.clone();

        Button::new("sftp-btn-upload")
            .custom(style)
            .xsmall()
            .w(ICON_BUTTON_SIZE)
            .h(ICON_BUTTON_SIZE)
            .p_0()
            .rounded(px(6.))
            .tooltip(i18n::t(lang, "sftp.toolbar.upload"))
            .child(icon_sized(icons::UPLOAD, 14., p.text_muted))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let on_file = on_file.clone();
                let on_folder = on_folder.clone();
                menu.min_w(px(140.))
                    .item(
                        menu_item_element(icons::FILE, &upload_file_label).on_click(
                            move |_, window, cx| {
                                on_file(SftpToolbarEvent::UploadFile, window, cx);
                            },
                        ),
                    )
                    .item(
                        menu_item_element(icons::FOLDER, &upload_folder_label).on_click(
                            move |_, window, cx| {
                                on_folder(SftpToolbarEvent::UploadFolder, window, cx);
                            },
                        ),
                    )
            })
            .into_any_element()
    } else {
        icon_button_enabled(
            "sftp-btn-upload-disabled",
            icons::UPLOAD,
            i18n::t(lang, "sftp.toolbar.upload"),
            false,
            p,
        )
        .into_any_element()
    };

    panel_header(p)
        .bg(p.background)
        .px(px(6.))
        .gap(px(2.))
        .child(toolbar_button(
            "sftp-btn-back",
            icons::ARROW_LEFT,
            "sftp.toolbar.back",
            can_back,
            SftpToolbarEvent::GoBack,
            &on_event,
            lang,
            p,
        ))
        .child(toolbar_button(
            "sftp-btn-forward",
            icons::ARROW_RIGHT,
            "sftp.toolbar.forward",
            can_forward,
            SftpToolbarEvent::GoForward,
            &on_event,
            lang,
            p,
        ))
        .child(toolbar_button(
            "sftp-btn-up",
            icons::ARROW_UP,
            "sftp.toolbar.up",
            can_up,
            SftpToolbarEvent::GoUp,
            &on_event,
            lang,
            p,
        ))
        // 路径框占满中间
        .child(
            div()
                .flex_1()
                .min_w_0()
                .mx(px(4.))
                .flex()
                .children(path_bar_state.filter(|_| connected)),
        )
        .child(filter_toggle)
        .child(toolbar_button(
            "sftp-btn-new-folder",
            icons::FOLDER_PLUS,
            "sftp.toolbar.new_folder",
            connected,
            SftpToolbarEvent::NewFolder,
            &on_event,
            lang,
            p,
        ))
        .child(upload_control)
        .child(toolbar_button(
            "sftp-btn-download",
            icons::DOWNLOAD,
            "sftp.toolbar.download",
            connected && has_selection,
            SftpToolbarEvent::Download,
            &on_event,
            lang,
            p,
        ))
}
