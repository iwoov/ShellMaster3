// SFTP 内置代码编辑器面板组件

use std::path::Path;

use gpui_kit::component::input::{Editor, Redo, RopeExt as _, Undo};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::common::button::{
    icon_button_colored, icon_button_custom, icon_button_enabled, icon_button_sized,
    icon_toggle_button, text_button, text_tooltip, ButtonKind, IconButtonLook,
};
use crate::components::common::dialog::{
    dialog_footer, dialog_header, dialog_panel, DialogOverlay,
};
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{panel_header, spinner, status_dot, HEADER_CONTROL_HEIGHT};
use crate::constants::icons;
use crate::i18n;
use crate::keybindings::{CloseEditorTab, SaveFile};
use crate::models::settings::Language;
use crate::state::{
    CenterViewMode, EditorBufferStatus, SessionState, SessionStatus, SessionTab, SftpEditorTab,
};
use crate::theme::Palette;

/// 编辑器文件标签的最大名称宽度
const EDITOR_TAB_LABEL_MAX: Pixels = px(160.);
/// 编辑器文件标签悬停分组名
const EDITOR_TAB_GROUP: &str = "sftp-editor-tab";
/// 终端标签悬停分组名（与 terminal_page 保持一致）
const TERMINAL_TAB_GROUP: &str = "terminal-tab";

/// 根据文件名选择对应文件图标
fn editor_file_icon(file_name: &str) -> &'static str {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let lower = file_name.to_ascii_lowercase();

    if matches!(ext.as_str(), "json" | "jsonc" | "toml" | "yml" | "yaml") {
        icons::FILE_JSON
    } else if matches!(
        ext.as_str(),
        "rs" | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "go"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "php"
            | "lua"
            | "cs"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "sql"
            | "html"
            | "css"
            | "scss"
            | "xml"
    ) || matches!(lower.as_str(), "dockerfile" | "makefile" | "nginx.conf")
    {
        icons::FILE_CODE
    } else if matches!(
        ext.as_str(),
        "md" | "txt" | "log" | "conf" | "cfg" | "ini" | "service" | "env"
    ) || lower.starts_with(".env")
        || lower.starts_with(".bash")
        || lower.starts_with(".zsh")
    {
        icons::FILE_TEXT
    } else {
        icons::FILE
    }
}

/// 渲染编辑器文件标签条（同时复用于终端顶部栏与编辑器顶部栏）
pub fn render_editor_tabs_strip(
    tab: &SessionTab,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> impl IntoElement {
    let close_tooltip = i18n::t(lang, "sftp.editor.close_tab");
    let in_editor_view = tab.center_view == CenterViewMode::Editor;

    div()
        .id("sftp-editor-tabs-strip")
        .flex()
        .items_center()
        .gap(px(2.))
        .min_w_0()
        .overflow_x_scroll()
        .children(tab.editor_tabs.iter().map(|ed| {
            let is_active =
                in_editor_view && tab.active_editor_tab_id.as_deref() == Some(ed.id.as_str());
            let hover_bg = p.hover;
            let text_color = if is_active { p.text } else { p.text_muted };
            let icon_color = if is_active { p.primary } else { p.text_faint };

            let activate = {
                let session_state = session_state.clone();
                let tab_id = tab.id.clone();
                let editor_id = ed.id.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    session_state.update(cx, |state, cx| {
                        state.sftp_activate_editor_tab(&tab_id, &editor_id);
                        cx.notify();
                    });
                }
            };

            let close = {
                let session_state = session_state.clone();
                let tab_id = tab.id.clone();
                let editor_id = ed.id.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                    session_state.update(cx, |state, cx| {
                        state.sftp_request_close_editor_tab(&tab_id, &editor_id, cx);
                    });
                }
            };

            let remote_path_tooltip = SharedString::from(ed.remote_path.clone());
            let file_name = SharedString::from(ed.file_name.clone());
            let is_loading = matches!(ed.status, EditorBufferStatus::Loading);
            let is_modified = ed.is_modified;

            div()
                .id(SharedString::from(format!("sftp-editor-tab-{}", ed.id)))
                .group(EDITOR_TAB_GROUP)
                .flex_none()
                .h(HEADER_CONTROL_HEIGHT)
                .pl(px(9.))
                .pr(px(4.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .tooltip(text_tooltip(remote_path_tooltip))
                .when(is_active, |el| el.bg(p.selected))
                .when(!is_active, |el| el.hover(move |s| s.bg(hover_bg)))
                .on_click(activate)
                .child(if is_loading {
                    spinner(
                        SharedString::from(format!("editor-tab-spin-{}", ed.id)),
                        12.,
                        p.primary,
                    )
                    .into_any_element()
                } else {
                    icon_sized(editor_file_icon(&ed.file_name), 13., icon_color).into_any_element()
                })
                .child(
                    div()
                        .max_w(EDITOR_TAB_LABEL_MAX)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(12.))
                        .font_weight(if is_active {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(text_color)
                        .child(file_name),
                )
                .when(is_modified, |row| row.child(status_dot(p.warning, 6.)))
                .child(
                    icon_button_sized(
                        SharedString::from(format!("close-editor-tab-{}", ed.id)),
                        icons::X,
                        px(18.),
                        11.,
                        close_tooltip,
                        p,
                    )
                    .rounded(px(5.))
                    .when(!is_active && !is_modified, |btn| {
                        btn.invisible()
                            .group_hover(EDITOR_TAB_GROUP, |s| s.visible())
                    })
                    .on_click(close),
                )
        }))
}

/// 渲染编辑器视图顶部的工作区标签栏（与终端顶部栏像素对齐：终端标签 + 新建终端 + 分隔线 + 文件标签）
fn render_workspace_header_for_editor(
    tab: &SessionTab,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> impl IntoElement {
    let label_prefix = i18n::t(lang, "session.terminal.tab_label");
    let can_close_terminal = tab.terminals.len() > 1;

    let terminal_tabs = tab.terminals.iter().map(|instance| {
        let label = format!("{} {}", label_prefix, instance.index);
        let hover_bg = p.hover;

        let activate = {
            let session_state = session_state.clone();
            let tab_id = tab.id.clone();
            let terminal_id = instance.id.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                session_state.update(cx, |state, cx| {
                    state.activate_terminal_instance(&tab_id, &terminal_id);
                    cx.notify();
                });
            }
        };

        div()
            .id(SharedString::from(format!(
                "editor-header-terminal-tab-{}",
                instance.id
            )))
            .group(TERMINAL_TAB_GROUP)
            .flex_none()
            .h(HEADER_CONTROL_HEIGHT)
            .pl(px(10.))
            .pr(px(if can_close_terminal { 4. } else { 10. }))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(activate)
            .child(icon_sized(icons::TERMINAL, 13., p.text_faint))
            .child(
                div()
                    .max_w(px(160.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(label),
            )
            .when(can_close_terminal, |row| {
                let session_state = session_state.clone();
                let tab_id = tab.id.clone();
                let terminal_id = instance.id.clone();
                row.child(
                    icon_button_sized(
                        SharedString::from(format!("editor-header-close-terminal-{}", instance.id)),
                        icons::X,
                        px(18.),
                        11.,
                        i18n::t(lang, "terminal.tab.close"),
                        p,
                    )
                    .rounded(px(5.))
                    .invisible()
                    .group_hover(TERMINAL_TAB_GROUP, |s| s.visible())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        session_state.update(cx, |state, cx| {
                            state.close_terminal_instance(&tab_id, &terminal_id, cx);
                            cx.notify();
                        });
                    }),
                )
            })
    });

    let add_terminal_btn = {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        icon_button_sized(
            "editor-header-add-terminal-btn",
            icons::PLUS,
            px(26.),
            13.,
            i18n::t(lang, "terminal.tab.new"),
            p,
        )
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.add_terminal_instance(&tab_id);
                cx.notify();
            });
        })
    };

    panel_header(p)
        .id("editor-workspace-toolbar")
        .w_full()
        .px(px(6.))
        .gap(px(4.))
        .bg(p.background)
        .children(terminal_tabs)
        .child(add_terminal_btn)
        .child(
            div()
                .flex_none()
                .w(px(1.))
                .h(px(14.))
                .mx(px(2.))
                .bg(p.border),
        )
        .child(div().flex_1().min_w_0().child(render_editor_tabs_strip(
            tab,
            session_state,
            lang,
            p,
        )))
}

/// 渲染编辑器操作工具栏（路径复制 + 保存/重载/撤销/重做/查找/换行/外置打开）
fn render_editor_toolbar(
    tab: &SessionTab,
    active_ed: &SftpEditorTab,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let is_connected = matches!(tab.status, SessionStatus::Connected);
    let ready_editor = match &active_ed.status {
        EditorBufferStatus::Ready { editor_state } => Some(editor_state.clone()),
        _ => None,
    };
    let can_save =
        ready_editor.is_some() && active_ed.is_modified && !active_ed.is_saving && is_connected;
    let can_edit_actions = ready_editor.is_some();

    let copy_key = format!("editor-path-{}", active_ed.id);
    let is_copied = session_state.read(cx).is_copied(&copy_key);

    let save_shortcut = if cfg!(target_os = "macos") {
        "Cmd+S"
    } else {
        "Ctrl+S"
    };
    let save_tooltip = format!("{} ({})", i18n::t(lang, "sftp.editor.save"), save_shortcut);

    // 路径复制区
    let path_display = {
        let session_state = session_state.clone();
        let remote_path = active_ed.remote_path.clone();
        let hover_bg = p.hover;
        div()
            .id(SharedString::from(format!(
                "editor-path-copy-{}",
                active_ed.id
            )))
            .flex_1()
            .min_w_0()
            .h(px(24.))
            .px(px(6.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .tooltip(text_tooltip(i18n::t(lang, "sftp.editor.copy_path")))
            .on_click(move |_, _, cx| {
                SessionState::copy_with_feedback(
                    &session_state,
                    copy_key.clone(),
                    remote_path.clone(),
                    cx,
                );
            })
            .child(icon_sized(
                if is_copied { icons::CHECK } else { icons::COPY },
                12.,
                if is_copied { p.success } else { p.text_faint },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(active_ed.remote_path.clone()),
            )
            .when(active_ed.is_modified, |row| {
                row.child(
                    div()
                        .flex_none()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(4.))
                        .bg(p.warning.opacity(0.14))
                        .text_size(px(11.))
                        .text_color(p.warning)
                        .child(i18n::t(lang, "sftp.editor.unsaved_badge")),
                )
            })
    };

    // 保存按钮
    let save_btn = if active_ed.is_saving {
        div()
            .id("editor-saving-indicator")
            .size(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .tooltip(text_tooltip(i18n::t(lang, "sftp.editor.saving")))
            .child(spinner("editor-save-spinner", 13., p.primary))
            .into_any_element()
    } else {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        let ed_id = active_ed.id.clone();
        let btn = icon_button_custom(
            "editor-save-btn",
            icons::SAVE,
            IconButtonLook {
                size: px(26.),
                icon_size: 14.,
                icon_color: if can_save { p.primary } else { p.text_muted },
                hover_icon_color: p.primary,
                rest_bg: if can_save {
                    Some(p.primary.opacity(0.12))
                } else {
                    None
                },
                enabled: can_save,
            },
            save_tooltip,
            p,
        );
        if can_save {
            btn.on_click(move |_, _, cx| {
                session_state.update(cx, |state, cx| {
                    state.sftp_save_editor_tab(&tab_id, &ed_id, false, cx);
                });
            })
            .into_any_element()
        } else {
            btn.into_any_element()
        }
    };

    // 重新加载按钮
    let reload_btn = {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        let ed_id = active_ed.id.clone();
        let enabled = !matches!(active_ed.status, EditorBufferStatus::Loading)
            && !active_ed.is_saving
            && is_connected;
        let btn = icon_button_custom(
            "editor-reload-btn",
            icons::REFRESH,
            IconButtonLook {
                size: px(26.),
                icon_size: 13.,
                icon_color: p.text_muted,
                hover_icon_color: p.text,
                rest_bg: None,
                enabled,
            },
            i18n::t(lang, "sftp.editor.reload"),
            p,
        );
        if enabled {
            btn.on_click(move |_, _, cx| {
                session_state.update(cx, |state, cx| {
                    state.sftp_reload_editor_tab(&tab_id, &ed_id, false, cx);
                });
            })
        } else {
            btn
        }
    };

    // 撤销按钮
    let undo_btn = {
        let editor_opt = ready_editor.clone();
        let btn = icon_button_enabled(
            "editor-undo-btn",
            icons::UNDO,
            i18n::t(lang, "sftp.editor.undo"),
            can_edit_actions,
            p,
        )
        .size(px(26.));
        if let Some(editor) = editor_opt {
            btn.on_click(move |_, window, cx| {
                editor.update(cx, |s, cx| s.focus(window, cx));
                window.dispatch_action(Box::new(Undo), cx);
            })
        } else {
            btn
        }
    };

    // 重做按钮
    let redo_btn = {
        let editor_opt = ready_editor.clone();
        let btn = icon_button_enabled(
            "editor-redo-btn",
            icons::REDO,
            i18n::t(lang, "sftp.editor.redo"),
            can_edit_actions,
            p,
        )
        .size(px(26.));
        if let Some(editor) = editor_opt {
            btn.on_click(move |_, window, cx| {
                editor.update(cx, |s, cx| s.focus(window, cx));
                window.dispatch_action(Box::new(Redo), cx);
            })
        } else {
            btn
        }
    };

    // 查找/替换按钮
    let search_btn = {
        let editor_opt = ready_editor.clone();
        let btn = icon_button_enabled(
            "editor-search-btn",
            icons::SEARCH,
            i18n::t(lang, "sftp.editor.search"),
            can_edit_actions,
            p,
        )
        .size(px(26.));
        if let Some(editor) = editor_opt {
            btn.on_click(move |_, window, cx| {
                editor.update(cx, |s, cx| {
                    s.focus(window, cx);
                    s.open_search(false, cx);
                });
            })
        } else {
            btn
        }
    };

    // 自动换行切换按钮
    let wrap_btn = {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        let ed_id = active_ed.id.clone();
        icon_toggle_button(
            "editor-wrap-btn",
            icons::CODE,
            active_ed.soft_wrap,
            i18n::t(lang, "sftp.editor.word_wrap"),
            p,
        )
        .size(px(26.))
        .on_click(move |_, window, cx| {
            session_state.update(cx, |state, cx| {
                state.sftp_toggle_editor_soft_wrap(&tab_id, &ed_id, window, cx);
            });
        })
    };

    // 用外置编辑器打开按钮
    let external_btn = {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        let remote_path = active_ed.remote_path.clone();
        icon_button_colored(
            "editor-open-external-btn",
            icons::EDIT,
            p.text_muted,
            p.text,
            i18n::t(lang, "sftp.editor.open_external"),
            p,
        )
        .size(px(26.))
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.sftp_edit_file_external(&tab_id, remote_path.clone(), cx);
            });
        })
    };

    div()
        .flex_none()
        .h(px(32.))
        .w_full()
        .px(px(8.))
        .bg(p.surface)
        .border_b_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .gap(px(4.))
        .child(path_display)
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(save_btn)
                .child(reload_btn)
                .child(div().w(px(1.)).h(px(14.)).mx(px(3.)).bg(p.border))
                .child(undo_btn)
                .child(redo_btn)
                .child(search_btn)
                .child(wrap_btn)
                .child(div().w(px(1.)).h(px(14.)).mx(px(3.)).bg(p.border))
                .child(external_btn),
        )
}

/// 渲染底部状态栏（行数/断线提示 + 光标行列 + 编码 + 换行符 + 语法语言）
fn render_editor_statusbar(
    tab: &SessionTab,
    active_ed: &SftpEditorTab,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let is_connected = matches!(tab.status, SessionStatus::Connected);

    let (cursor_text, lines_count) = match &active_ed.status {
        EditorBufferStatus::Ready { editor_state } => {
            let st = editor_state.read(cx);
            let pos = st.cursor_position();
            let line_count = st.text().lines_len().max(1);
            (
                Some(format!("Ln {}, Col {}", pos.line + 1, pos.character + 1)),
                Some(line_count),
            )
        }
        _ => (None, None),
    };

    let encoding_label = if active_ed.has_bom {
        "UTF-8 BOM"
    } else {
        "UTF-8"
    };

    let left_info = if !is_connected {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_color(p.warning)
            .child(status_dot(p.warning, 6.))
            .child(i18n::t(lang, "sftp.editor.disconnected"))
            .into_any_element()
    } else if active_ed.is_saving {
        div()
            .text_color(p.primary)
            .child(i18n::t(lang, "sftp.editor.saving"))
            .into_any_element()
    } else if let Some(lines) = lines_count {
        div()
            .text_color(p.text_faint)
            .child(format!("{} {}", lines, i18n::t(lang, "sftp.editor.lines")))
            .into_any_element()
    } else {
        div().into_any_element()
    };

    div()
        .flex_none()
        .h(px(24.))
        .w_full()
        .px(px(10.))
        .bg(p.surface)
        .border_t_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(11.))
        .text_color(p.text_muted)
        .child(left_info)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .children(cursor_text.map(|t| div().child(t)))
                .child(div().child(encoding_label))
                .child(div().child(active_ed.line_ending.label()))
                .child(div().text_color(p.text).child(active_ed.language_name)),
        )
}

/// 渲染 SFTP 内置代码编辑器主面板（占据中间主区上半部分）
pub fn render_sftp_editor_panel(
    tab: &SessionTab,
    session_state: Entity<SessionState>,
    lang: &Language,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let active_ed = tab
        .active_editor_tab_id
        .as_deref()
        .and_then(|id| tab.editor_tabs.iter().find(|e| e.id == id))
        .or_else(|| tab.editor_tabs.first());

    let settings = crate::services::storage::cached_settings();
    let sftp_settings = &settings.sftp;
    let font_family = if sftp_settings.editor_font_family.trim().is_empty() {
        "JetBrains Mono".to_string()
    } else {
        sftp_settings.editor_font_family.clone()
    };
    let font_size = sftp_settings.editor_font_size.clamp(10, 32) as f32;
    let line_height = sftp_settings.editor_line_height.clamp(1.0, 2.5);

    let mut root = div()
        .id("sftp-editor-panel")
        .key_context("SftpEditor")
        .size_full()
        .flex()
        .flex_col()
        .bg(p.background)
        .child(render_workspace_header_for_editor(
            tab,
            session_state.clone(),
            lang,
            &p,
        ));

    if let Some(ed) = active_ed {
        // 绑定局部快捷键：Cmd+S / Ctrl+S 保存，Cmd+W / Ctrl+W 关闭当前文件标签
        let tab_id_save = tab.id.clone();
        let ed_id_save = ed.id.clone();
        let state_save = session_state.clone();
        let tab_id_close = tab.id.clone();
        let ed_id_close = ed.id.clone();
        let state_close = session_state.clone();

        root = root
            .on_action(move |_: &SaveFile, _, cx| {
                state_save.update(cx, |state, cx| {
                    state.sftp_save_editor_tab(&tab_id_save, &ed_id_save, false, cx);
                });
            })
            .on_action(move |_: &CloseEditorTab, _, cx| {
                state_close.update(cx, |state, cx| {
                    state.sftp_request_close_editor_tab(&tab_id_close, &ed_id_close, cx);
                });
            })
            .child(render_editor_toolbar(
                tab,
                ed,
                session_state.clone(),
                lang,
                &p,
                cx,
            ));

        // 编辑器主体内容
        let body: AnyElement = match &ed.status {
            EditorBufferStatus::Loading | EditorBufferStatus::PendingInit { .. } => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .child(spinner("editor-body-loading", 20., p.primary))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(i18n::t(lang, "sftp.editor.loading")),
                )
                .into_any_element(),
            EditorBufferStatus::Error(err) => {
                let state_retry = session_state.clone();
                let tab_id_retry = tab.id.clone();
                let ed_id_retry = ed.id.clone();
                let state_close_err = session_state.clone();
                let tab_id_close_err = tab.id.clone();
                let ed_id_close_err = ed.id.clone();

                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .px(px(24.))
                    .child(icon_sized(icons::INFO, 24., p.danger))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.text)
                            .child(i18n::t(lang, "sftp.editor.load_failed")),
                    )
                    .child(
                        div()
                            .max_w(px(460.))
                            .text_center()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(err.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                text_button(
                                    "editor-retry-load",
                                    ButtonKind::Primary,
                                    i18n::t(lang, "sftp.editor.retry"),
                                    &p,
                                )
                                .on_click(move |_, _, cx| {
                                    state_retry.update(cx, |state, cx| {
                                        state.sftp_reload_editor_tab(
                                            &tab_id_retry,
                                            &ed_id_retry,
                                            true,
                                            cx,
                                        );
                                    });
                                }),
                            )
                            .child(
                                text_button(
                                    "editor-close-err-tab",
                                    ButtonKind::Secondary,
                                    i18n::t(lang, "common.close"),
                                    &p,
                                )
                                .on_click(move |_, _, cx| {
                                    state_close_err.update(cx, |state, cx| {
                                        state.sftp_force_close_editor_tab(
                                            &tab_id_close_err,
                                            &ed_id_close_err,
                                            cx,
                                        );
                                    });
                                }),
                            ),
                    )
                    .into_any_element()
            }
            EditorBufferStatus::Ready { editor_state } => div()
                .size_full()
                .overflow_hidden()
                .bg(p.background)
                .child(
                    Editor::new(editor_state)
                        .h(relative(1.))
                        .w_full()
                        .appearance(false)
                        .bordered(false)
                        .font_family(SharedString::from(font_family))
                        .text_size(px(font_size))
                        .line_height(relative(line_height)),
                )
                .into_any_element(),
        };

        root = root
            .child(div().flex_1().min_h(px(0.)).w_full().child(body))
            .child(render_editor_statusbar(tab, ed, lang, &p, cx));
    }

    root
}

/// 渲染编辑器未保存确认弹窗（关闭单文件 / 重载单文件 / 关闭含未保存文件的会话）
pub fn render_editor_confirm_dialogs(
    session_state: Entity<SessionState>,
    lang: &Language,
    cx: &App,
) -> Option<AnyElement> {
    let p = crate::theme::palette(cx);
    let state = session_state.read(cx);

    // 1. 关闭单个未保存文件确认弹窗
    if let Some((tab_id, editor_id)) = state.pending_close_editor_tab.clone() {
        let file_name = state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.editor_tabs.iter().find(|e| e.id == editor_id))
            .map(|e| e.file_name.clone())
            .unwrap_or_default();
        let is_connected = state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .is_some_and(|t| matches!(t.status, SessionStatus::Connected));

        let cancel_state = session_state.clone();
        let cancel_fn = move |_: &mut Window, cx: &mut App| {
            cancel_state.update(cx, |s, cx| {
                s.pending_close_editor_tab = None;
                cx.notify();
            });
        };

        let discard_state = session_state.clone();
        let tab_id_discard = tab_id.clone();
        let ed_id_discard = editor_id.clone();

        let save_state = session_state.clone();
        let tab_id_save = tab_id.clone();
        let ed_id_save = editor_id.clone();

        let desc = SharedString::from(
            i18n::t(lang, "sftp.editor.unsaved_close_message").replace("{name}", &file_name),
        );

        let panel = dialog_panel(&p)
            .w(px(420.))
            .child(dialog_header(
                i18n::t(lang, "sftp.editor.unsaved_title"),
                Some(desc),
                i18n::t(lang, "common.close"),
                cancel_fn.clone(),
                &p,
            ))
            .child(
                dialog_footer(None, &p)
                    .child(
                        text_button(
                            "editor-unsaved-cancel",
                            ButtonKind::Secondary,
                            i18n::t(lang, "common.cancel"),
                            &p,
                        )
                        .on_click({
                            let cancel_fn = cancel_fn.clone();
                            move |_, window, cx| cancel_fn(window, cx)
                        }),
                    )
                    .child(
                        text_button(
                            "editor-unsaved-discard",
                            ButtonKind::Danger,
                            i18n::t(lang, "sftp.editor.discard_changes"),
                            &p,
                        )
                        .on_click(move |_, _, cx| {
                            discard_state.update(cx, |s, cx| {
                                s.sftp_force_close_editor_tab(&tab_id_discard, &ed_id_discard, cx);
                            });
                        }),
                    )
                    .when(is_connected, |footer| {
                        footer.child(
                            text_button(
                                "editor-unsaved-save-close",
                                ButtonKind::Primary,
                                i18n::t(lang, "sftp.editor.save_and_close"),
                                &p,
                            )
                            .on_click(move |_, _, cx| {
                                save_state.update(cx, |s, cx| {
                                    s.pending_close_editor_tab = None;
                                    s.sftp_save_editor_tab(&tab_id_save, &ed_id_save, true, cx);
                                });
                            }),
                        )
                    }),
            );

        return Some(
            DialogOverlay::new()
                .on_escape(cancel_fn.clone())
                .on_backdrop_click(cancel_fn)
                .render("editor-unsaved-close-overlay", panel, &p)
                .into_any_element(),
        );
    }

    // 2. 重新加载未保存文件确认弹窗
    if let Some((tab_id, editor_id)) = state.pending_reload_editor_tab.clone() {
        let file_name = state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.editor_tabs.iter().find(|e| e.id == editor_id))
            .map(|e| e.file_name.clone())
            .unwrap_or_default();

        let cancel_state = session_state.clone();
        let cancel_fn = move |_: &mut Window, cx: &mut App| {
            cancel_state.update(cx, |s, cx| {
                s.pending_reload_editor_tab = None;
                cx.notify();
            });
        };

        let reload_state = session_state.clone();
        let desc = SharedString::from(
            i18n::t(lang, "sftp.editor.unsaved_reload_message").replace("{name}", &file_name),
        );

        let panel = dialog_panel(&p)
            .w(px(400.))
            .child(dialog_header(
                i18n::t(lang, "sftp.editor.reload_confirm_title"),
                Some(desc),
                i18n::t(lang, "common.close"),
                cancel_fn.clone(),
                &p,
            ))
            .child(
                dialog_footer(None, &p)
                    .child(
                        text_button(
                            "editor-reload-cancel",
                            ButtonKind::Secondary,
                            i18n::t(lang, "common.cancel"),
                            &p,
                        )
                        .on_click({
                            let cancel_fn = cancel_fn.clone();
                            move |_, window, cx| cancel_fn(window, cx)
                        }),
                    )
                    .child(
                        text_button(
                            "editor-reload-confirm",
                            ButtonKind::Danger,
                            i18n::t(lang, "sftp.editor.discard_and_reload"),
                            &p,
                        )
                        .on_click(move |_, _, cx| {
                            reload_state.update(cx, |s, cx| {
                                s.pending_reload_editor_tab = None;
                                s.sftp_reload_editor_tab(&tab_id, &editor_id, true, cx);
                            });
                        }),
                    ),
            );

        return Some(
            DialogOverlay::new()
                .on_escape(cancel_fn.clone())
                .on_backdrop_click(cancel_fn)
                .render("editor-unsaved-reload-overlay", panel, &p)
                .into_any_element(),
        );
    }

    // 3. 关闭含有未保存文件的 SSH 会话确认弹窗
    if let Some(tab_id) = state.pending_close_session_tab.clone() {
        let unsaved_count = state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.editor_tabs.iter().filter(|e| e.is_modified).count())
            .unwrap_or(0);

        let cancel_state = session_state.clone();
        let cancel_fn = move |_: &mut Window, cx: &mut App| {
            cancel_state.update(cx, |s, cx| {
                s.pending_close_session_tab = None;
                cx.notify();
            });
        };

        let close_state = session_state.clone();
        let desc = SharedString::from(
            i18n::t(lang, "sftp.editor.session_unsaved_message")
                .replace("{count}", &unsaved_count.to_string()),
        );

        let panel = dialog_panel(&p)
            .w(px(400.))
            .child(dialog_header(
                i18n::t(lang, "sftp.editor.unsaved_title"),
                Some(desc),
                i18n::t(lang, "common.close"),
                cancel_fn.clone(),
                &p,
            ))
            .child(
                dialog_footer(None, &p)
                    .child(
                        text_button(
                            "session-unsaved-cancel",
                            ButtonKind::Secondary,
                            i18n::t(lang, "common.cancel"),
                            &p,
                        )
                        .on_click({
                            let cancel_fn = cancel_fn.clone();
                            move |_, window, cx| cancel_fn(window, cx)
                        }),
                    )
                    .child(
                        text_button(
                            "session-unsaved-force-close",
                            ButtonKind::Danger,
                            i18n::t(lang, "sftp.editor.force_close_session"),
                            &p,
                        )
                        .on_click(move |_, _, cx| {
                            close_state.update(cx, |s, cx| {
                                s.pending_close_session_tab = None;
                                s.close_tab(&tab_id);
                                cx.notify();
                            });
                        }),
                    ),
            );

        return Some(
            DialogOverlay::new()
                .on_escape(cancel_fn.clone())
                .on_backdrop_click(cancel_fn)
                .render("session-unsaved-close-overlay", panel, &p)
                .into_any_element(),
        );
    }

    None
}
