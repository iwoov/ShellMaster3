use crate::components::common::icon::render_icon;
use crate::constants::icons;
use gpui::*;
use gpui_component::ActiveTheme;

pub fn render_windows_controls(cx: &App) -> impl IntoElement {
    // macOS uses native traffic lights, so we don't render anything
    if cfg!(target_os = "macos") {
        return div().into_any_element();
    }

    let icon_color = cx.theme().foreground;
    // Standard Windows caption button width is usually 46px
    let button_width = px(46.);

    div()
        .flex()
        .items_center()
        .h_full()
        // Minimize Button - using window_control_area for native platform behavior
        .child(
            div()
                .id("minimize-btn")
                .w(button_width)
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_default()
                .hover(|s| s.bg(rgba(0x80808040)))
                .window_control_area(WindowControlArea::Min)
                .child(
                    // Draw a horizontal line (10px width)
                    div()
                        .w(px(10.))
                        .h(px(1.)) // 1px thick
                        .bg(icon_color),
                ),
        )
        // Maximize/Restore Button - using window_control_area for native platform behavior
        .child(
            div()
                .id("maximize-btn")
                .w(button_width)
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_default()
                .hover(|s| s.bg(rgba(0x80808040)))
                .window_control_area(WindowControlArea::Max)
                .child(
                    // Draw a square outline (10px size)
                    div()
                        .w(px(10.))
                        .h(px(10.))
                        .border_1()
                        .border_color(icon_color),
                ),
        )
        // Close Button - using window_control_area for native platform behavior
        .child(
            div()
                .id("close-btn")
                .w(button_width)
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_default()
                // Red background on hover for close button
                .hover(|s| s.bg(red()))
                .window_control_area(WindowControlArea::Close)
                .child(
                    // Use existing X icon, scale it down slightly if needed
                    render_icon(icons::X, icon_color.into()),
                ),
        )
        .into_any_element()
}
