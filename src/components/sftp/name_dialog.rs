// 新建文件、新建文件夹共用的弹窗：在当前目录下输入一个名称

use gpui::*;
use gpui_component::input::InputState;

use crate::components::common::button::{button, text_button, ButtonKind};
use crate::components::common::dialog::{
    dialog_footer, dialog_header, dialog_panel, DialogOverlay,
};
use crate::components::common::input::{field_error, form_field, text_field};
use crate::i18n;
use crate::models::settings::Language;

/// 弹窗宽度
const DIALOG_WIDTH: Pixels = px(420.);

/// 弹窗显示的内容
pub struct NameDialogProps<'a> {
    pub id: &'static str,
    pub lang: &'a Language,
    pub title: &'static str,
    pub label: &'static str,
    /// 新文件 / 文件夹所在的目录
    pub directory: &'a str,
    pub input: Option<&'a Entity<InputState>>,
    pub error: Option<SharedString>,
    /// 正在创建：主按钮显示加载中并且不可点
    pub is_creating: bool,
    pub focus_handle: Option<&'a FocusHandle>,
}

/// 渲染弹窗。回车或点「创建」调用 `on_submit`（创建中不响应），Esc、关闭、取消调用 `on_cancel`。
pub fn render_name_dialog(
    props: NameDialogProps,
    on_cancel: impl Fn(&mut Window, &mut App) + Clone + 'static,
    on_submit: impl Fn(&mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> Stateful<Div> {
    let p = crate::theme::palette(cx);
    let lang = props.lang;
    let has_error = props.error.is_some();
    let location = i18n::t(lang, "sftp.new_item.location").replace("{}", props.directory);
    let is_creating = props.is_creating;

    let submit = {
        let on_submit = on_submit.clone();
        move |window: &mut Window, cx: &mut App| {
            if !is_creating {
                on_submit(window, cx);
            }
        }
    };
    let submit_for_button = submit.clone();
    let on_cancel_for_close = on_cancel.clone();
    let on_cancel_for_button = on_cancel.clone();

    let panel = dialog_panel(&p)
        .w(DIALOG_WIDTH)
        .child(dialog_header(
            props.title,
            Some(location.into()),
            i18n::t(lang, "common.close"),
            on_cancel_for_close,
            &p,
        ))
        .child(div().px(px(20.)).pt(px(12.)).child(form_field(
            props.label,
            match props.input {
                Some(input) => field_error(text_field(input), has_error, &p).into_any_element(),
                None => div().into_any_element(),
            },
            props.error,
            &p,
        )))
        .child(
            dialog_footer(None, &p)
                .child(
                    text_button(
                        SharedString::from(format!("{}-cancel-btn", props.id)),
                        ButtonKind::Secondary,
                        i18n::t(lang, "common.cancel"),
                        &p,
                    )
                    .on_click(move |_, window, cx| on_cancel_for_button(window, cx)),
                )
                .child(
                    button(
                        SharedString::from(format!("{}-create-btn", props.id)),
                        ButtonKind::Primary,
                        None,
                        if is_creating {
                            i18n::t(lang, "sftp.new_item.creating")
                        } else {
                            i18n::t(lang, "sftp.new_item.create")
                        },
                        !is_creating,
                        &p,
                    )
                    .on_click(move |_, window, cx| submit_for_button(window, cx)),
                ),
        );

    DialogOverlay::new()
        .focus(props.focus_handle)
        .on_escape(on_cancel)
        .on_enter(submit)
        .render(
            SharedString::from(format!("{}-overlay", props.id)),
            panel,
            &p,
        )
}
