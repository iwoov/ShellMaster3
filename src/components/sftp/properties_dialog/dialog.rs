// 属性对话框渲染

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;

use super::state::PropertiesDialogState;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::FileType;
use crate::services::storage;

/// 渲染属性对话框覆盖层
pub fn render_properties_dialog_overlay(
    state: Entity<PropertiesDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_data = state.read(cx);

    // 如果没有 entry，返回空
    let entry = match &state_data.entry {
        Some(e) => e.clone(),
        None => return div().into_any_element(),
    };

    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    // 主题颜色
    let dialog_bg = cx.theme().popover;
    let border_color = cx.theme().border;
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let label_color = cx.theme().muted_foreground;
    let value_color = cx.theme().foreground;
    let section_bg = cx.theme().secondary;
    let bg_overlay = hsla(0.0, 0.0, 0.0, 0.5);

    // 获取动态数据
    let symlink_target = state_data.symlink_target.clone();
    let folder_size_display = state_data.format_folder_size();
    let is_folder = entry.is_dir();
    let is_symlink = entry.file_type == FileType::Symlink;

    // 格式化修改时间
    let modified_str = entry
        .modified
        .map(|t| {
            let datetime: chrono::DateTime<chrono::Local> = t.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "-".to_string());

    // 权限格式化（同时显示符号和八进制）
    let perms_display = format!(
        "{} ({:o})",
        entry.format_permissions(),
        entry.permissions & 0o777
    );

    // 标题
    let title = i18n::t(&lang, "sftp.properties.title");

    // 标签文本
    let type_label = i18n::t(&lang, "sftp.properties.type");
    let path_label = i18n::t(&lang, "sftp.properties.path");
    let size_label = i18n::t(&lang, "sftp.properties.size");
    let modified_label = i18n::t(&lang, "sftp.properties.modified");
    let permissions_label = i18n::t(&lang, "sftp.properties.permissions");
    let link_target_label = i18n::t(&lang, "sftp.properties.link_target");

    // 类型显示
    let type_display = match entry.file_type {
        FileType::Directory => i18n::t(&lang, "sftp.properties.type_folder"),
        FileType::File => i18n::t(&lang, "sftp.properties.type_file"),
        FileType::Symlink => i18n::t(&lang, "sftp.properties.type_symlink"),
        FileType::Other => i18n::t(&lang, "sftp.properties.type_other"),
    };

    // 大小显示
    let size_display = if is_folder {
        folder_size_display
    } else {
        entry.format_size()
    };

    let state_for_close = state.clone();
    let state_for_backdrop = state.clone();

    div()
        .id("properties-dialog-overlay")
        .absolute()
        .inset_0()
        .bg(bg_overlay)
        .flex()
        .items_start()
        .pt(px(100.))
        .justify_center()
        // 点击遮罩关闭
        .on_click(move |_, _, cx| {
            state_for_backdrop.update(cx, |s, _| s.close());
        })
        .child(
            div()
                .id("properties-dialog")
                .w(px(320.))
                .bg(dialog_bg)
                .border_1()
                .border_color(border_color)
                .rounded_lg()
                .shadow_lg()
                .overflow_hidden()
                .flex()
                .flex_col()
                // 阻止点击穿透
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                // 标题栏
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(border_color)
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(title_color)
                                .child(title.to_string()),
                        )
                        .child(
                            div()
                                .id("close-properties-dialog")
                                .w(px(20.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|s| s.bg(cx.theme().secondary_hover))
                                .on_click(move |_, _, cx| {
                                    state_for_close.update(cx, |s, _| s.close());
                                })
                                .child(svg().path(icons::X).size(px(14.)).text_color(label_color)),
                        ),
                )
                // 文件名区域
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(border_color)
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            // 文件图标
                            svg()
                                .path(match entry.file_type {
                                    FileType::Directory => icons::FOLDER,
                                    FileType::Symlink => icons::LINK,
                                    _ => icons::FILE,
                                })
                                .size(px(16.))
                                .text_color(label_color),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(value_color)
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(entry.name.clone()),
                        ),
                )
                // 内容区域 - 使用 section 样式
                .child(
                    div().p_3().child(
                        div()
                            .w_full()
                            .bg(section_bg)
                            .rounded(px(6.))
                            .overflow_hidden()
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    // 类型
                                    .child(render_property_row(
                                        &type_label,
                                        &type_display,
                                        label_color,
                                        value_color,
                                    ))
                                    // 完整路径
                                    .child(render_property_row(
                                        &path_label,
                                        &entry.path,
                                        label_color,
                                        value_color,
                                    ))
                                    // 大小
                                    .child(render_property_row(
                                        &size_label,
                                        &size_display,
                                        label_color,
                                        value_color,
                                    ))
                                    // 修改时间
                                    .child(render_property_row(
                                        &modified_label,
                                        &modified_str,
                                        label_color,
                                        value_color,
                                    ))
                                    // 权限
                                    .child(render_property_row(
                                        &permissions_label,
                                        &perms_display,
                                        label_color,
                                        value_color,
                                    ))
                                    // 符号链接目标（仅对符号链接显示）
                                    .when(is_symlink, |this| {
                                        let target = symlink_target.as_deref().unwrap_or("...");
                                        this.child(render_property_row(
                                            &link_target_label,
                                            target,
                                            label_color,
                                            value_color,
                                        ))
                                    }),
                            ),
                    ),
                ),
        )
        .into_any_element()
}

/// 渲染属性行
fn render_property_row(label: &str, value: &str, label_color: Hsla, value_color: Hsla) -> Div {
    div()
        .flex()
        .justify_between()
        .py(px(1.))
        .child(
            div()
                .text_xs()
                .text_color(label_color)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(value_color)
                .max_w(px(180.))
                .overflow_hidden()
                .text_ellipsis()
                .child(value.to_string()),
        )
}
