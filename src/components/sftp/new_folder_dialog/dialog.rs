// 新建文件夹对话框渲染组件

use gpui::*;

use super::super::name_dialog::{render_name_dialog, NameDialogProps};
use super::state::NewFolderDialogState;
use crate::i18n;

/// 渲染新建文件夹对话框覆盖层
pub fn render_new_folder_dialog_overlay<F>(
    state: Entity<NewFolderDialogState>,
    on_create: F,
    cx: &App,
) -> impl IntoElement
where
    F: Fn(String, String, &mut App) + Clone + 'static,
{
    let state_read = state.read(cx);
    let lang = state_read.lang.clone();

    let state_for_cancel = state.clone();
    let state_for_create = state.clone();

    render_name_dialog(
        NameDialogProps {
            id: "new-folder-dialog",
            lang: &lang,
            title: i18n::t(&lang, "sftp.new_folder.title"),
            label: i18n::t(&lang, "sftp.new_folder.name"),
            directory: &state_read.current_path,
            input: state_read.name_input.as_ref(),
            error: state_read.error_message.clone().map(SharedString::from),
            is_creating: state_read.is_creating,
            focus_handle: state_read.focus_handle.as_ref(),
        },
        move |_, cx| {
            state_for_cancel.update(cx, |s, cx| {
                s.close();
                cx.notify();
            });
        },
        move |_, cx| {
            let request = state_for_create.update(cx, |s, cx| {
                let request = s.validate_name(cx).then(|| {
                    s.start_creating();
                    (s.get_full_path(cx), s.tab_id.clone())
                });
                cx.notify();
                request
            });
            if let Some((full_path, tab_id)) = request {
                on_create(full_path, tab_id, cx);
            }
        },
        cx,
    )
}
