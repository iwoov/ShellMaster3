// 属性对话框渲染（只读）：文件图标 + 名称，下面一张卡片列出类型、路径、大小等

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::state::PropertiesDialogState;
use crate::components::common::button::icon_button;
use crate::components::common::dialog::{dialog_panel, DialogOverlay};
use crate::components::common::icon::icon_sized;
use crate::components::common::input::{settings_card, settings_row};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::sftp::FileType;
use crate::theme::Palette;

/// 弹窗宽度
const DIALOG_WIDTH: Pixels = px(400.);
/// 弹窗左右留白
const PADDING_X: Pixels = px(20.);
/// 属性行里留给标签的宽度（值从这之后开始，过长时截断）
const LABEL_RESERVE: Pixels = px(96.);

/// 渲染属性对话框覆盖层
pub fn render_properties_dialog_overlay(
    state: Entity<PropertiesDialogState>,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let state_data = state.read(cx);
    let Some(entry) = state_data.entry.clone() else {
        return div().into_any_element();
    };
    let p = crate::theme::palette(cx);
    let lang = state_data.lang.clone();
    let t = |key| i18n::t(&lang, key);

    let is_symlink = entry.file_type == FileType::Symlink;
    let type_display = match entry.file_type {
        FileType::Directory => t("sftp.properties.type_folder"),
        FileType::File => t("sftp.properties.type_file"),
        FileType::Symlink => t("sftp.properties.type_symlink"),
        FileType::Other => t("sftp.properties.type_other"),
    };
    let size_display = if entry.is_dir() {
        state_data.format_folder_size(&lang)
    } else {
        entry.format_size()
    };
    let modified = entry
        .modified
        .map(|time| {
            let datetime: chrono::DateTime<chrono::Local> = time.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "-".to_string());
    // 权限同时显示符号和八进制
    let permissions = format!(
        "{} ({:o})",
        entry.format_permissions(),
        entry.permissions & 0o777
    );
    let symlink_target = state_data
        .symlink_target
        .clone()
        .unwrap_or_else(|| "…".to_string());

    // 值的可用宽度：弹窗宽度去掉留白、卡片边框、行内边距、标签和间距
    let value_width = DIALOG_WIDTH - PADDING_X * 2. - px(2.) - px(28.) - px(16.) - LABEL_RESERVE;
    let row = |id: &'static str, label: &'static str, value: &str, mono: bool| {
        let value = if mono {
            fit.mono_label_with_tooltip(id, value, value_width, 12.)
        } else {
            fit.label_with_tooltip(id, value, value_width, 13., FontWeight::NORMAL)
        };
        settings_row(label, None, value.text_color(p.text), &p)
    };

    let card = settings_card(&p)
        .child(row(
            "properties-path",
            t("sftp.properties.path"),
            &entry.path,
            true,
        ))
        .child(row(
            "properties-size",
            t("sftp.properties.size"),
            &size_display,
            false,
        ))
        .child(row(
            "properties-modified",
            t("sftp.properties.modified"),
            &modified,
            false,
        ))
        .child(row(
            "properties-permissions",
            t("sftp.properties.permissions"),
            &permissions,
            true,
        ))
        .when(is_symlink, |card| {
            card.child(row(
                "properties-link-target",
                t("sftp.properties.link_target"),
                &symlink_target,
                true,
            ))
        });

    let state_for_close = state.clone();
    let state_for_escape = state.clone();
    let state_for_backdrop = state.clone();
    let close = move |state: &Entity<PropertiesDialogState>, cx: &mut App| {
        state.update(cx, |s, cx| {
            s.close();
            cx.notify();
        });
    };

    let panel = dialog_panel(&p)
        .w(DIALOG_WIDTH)
        .child(render_header(
            entry.file_type,
            &entry.name,
            type_display,
            fit,
            &p,
            icon_button("properties-close-btn", icons::X, t("common.close"), &p)
                .on_click(move |_, _, cx| close(&state_for_close, cx)),
        ))
        .child(div().px(PADDING_X).pt(px(4.)).pb(px(20.)).child(card));

    // 只读弹窗：点遮罩也可以关闭
    DialogOverlay::new()
        .focus(state_data.focus_handle.as_ref())
        .on_escape(move |_, cx| close(&state_for_escape, cx))
        .on_backdrop_click(move |_, cx| close(&state_for_backdrop, cx))
        .render("properties-dialog-overlay", panel, &p)
        .into_any_element()
}

/// 头部：文件类型图标块、名称和类型，右上角是关闭按钮
fn render_header(
    file_type: FileType,
    name: &str,
    type_display: &'static str,
    fit: &TextFitter,
    p: &Palette,
    close_button: impl IntoElement,
) -> Div {
    let icon = match file_type {
        FileType::Directory => icons::FOLDER,
        FileType::Symlink => icons::LINK,
        _ => icons::FILE,
    };
    // 名称可用宽度：弹窗宽度去掉左右留白、图标块、间距和关闭按钮
    let name_width = DIALOG_WIDTH - PADDING_X - px(12.) - px(36.) - px(12.) - px(12.) - px(28.);

    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .pl(PADDING_X)
        .pr(px(12.))
        .pt(px(16.))
        .pb(px(14.))
        .child(
            div()
                .flex_none()
                .size(px(36.))
                .rounded(px(8.))
                .bg(p.primary.opacity(0.12))
                .flex()
                .items_center()
                .justify_center()
                .child(icon_sized(icon, 18., p.primary)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    fit.label_with_tooltip(
                        "properties-name",
                        name,
                        name_width,
                        15.,
                        FontWeight::SEMIBOLD,
                    )
                    .text_color(p.text),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(type_display),
                ),
        )
        .child(close_button)
}
