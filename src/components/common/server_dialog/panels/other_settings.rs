// 其他设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

use super::super::ServerDialogState;

/// 渲染其他设置表单
pub fn render_other_settings_form(_state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    div().flex().flex_col().gap_3().child(
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(i18n::t(&lang, "server_dialog.no_other_settings")),
    )
}
