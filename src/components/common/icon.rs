// 图标组件

use gpui::*;

use crate::constants::icons;

/// 渲染 SVG 图标
pub fn render_icon(path: &'static str, color: Hsla) -> impl IntoElement {
    svg().path(path).size_4().text_color(color)
}

/// 渲染指定边长的 SVG 图标。返回 `Svg` 本身，调用方可以继续加样式（例如随分组悬停变色）
pub fn icon_sized(path: &'static str, size: f32, color: Hsla) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(color)
}

/// 发行版图标的品牌色。
///
/// 图标本身是单色路径，按这里的颜色着色；部分品牌原色在深色背景上几乎看不见
/// （Debian 的暗红、CentOS 的深紫、Alpine 的深蓝），深色模式下换成提亮后的同色系。
pub fn distribution_color(icon: &str, is_dark: bool) -> Hsla {
    let hex = match icon {
        icons::DISTRO_UBUNTU => 0xE95420,
        icons::DISTRO_DEBIAN if is_dark => 0xE5566D,
        icons::DISTRO_DEBIAN => 0xA81D33,
        icons::DISTRO_ROCKY if is_dark => 0x10B981,
        icons::DISTRO_ROCKY => 0x0E9F6E,
        icons::DISTRO_CENTOS if is_dark => 0x9A8FF5,
        icons::DISTRO_CENTOS => 0x262577,
        icons::DISTRO_FEDORA if is_dark => 0x51A2DA,
        icons::DISTRO_FEDORA => 0x3C8FCB,
        icons::DISTRO_ALPINE if is_dark => 0x4EA6D3,
        icons::DISTRO_ALPINE => 0x0D597F,
        icons::DISTRO_ARCH => 0x1793D1,
        icons::DISTRO_REDHAT if is_dark => 0xF05252,
        icons::DISTRO_REDHAT => 0xEE0000,
        icons::DISTRO_OPENSUSE if is_dark => 0x73BA25,
        icons::DISTRO_OPENSUSE => 0x5A9A14,
        _ => 0x8FA0B5,
    };
    rgb(hex).into()
}

/// 服务器图标及其颜色：识别出发行版时用品牌图标和品牌色，否则用通用服务器图标和 `fallback` 色
pub fn server_icon(
    distro: Option<&'static str>,
    is_dark: bool,
    fallback: Hsla,
) -> (&'static str, Hsla) {
    match distro {
        Some(icon) => (icon, distribution_color(icon, is_dark)),
        None => (icons::SERVER, fallback),
    }
}

/// 渲染 SVG/PNG 图标，保留原始颜色（用于品牌图标等）
pub fn render_colored_icon(path: &'static str, size: f32) -> impl IntoElement {
    img(path).w(px(size)).h(px(size))
}

/// 根据已采集的 OS 名称或连接配置文本识别 Linux 发行版图标。
pub fn distribution_icon(
    os_name: Option<&str>,
    name: &str,
    description: &str,
) -> Option<&'static str> {
    let source = format!("{} {} {}", os_name.unwrap_or_default(), name, description).to_lowercase();

    if source.contains("ubuntu") {
        Some(icons::DISTRO_UBUNTU)
    } else if source.contains("debian") {
        Some(icons::DISTRO_DEBIAN)
    } else if source.contains("rocky") {
        Some(icons::DISTRO_ROCKY)
    } else if source.contains("centos") {
        Some(icons::DISTRO_CENTOS)
    } else if source.contains("fedora") {
        Some(icons::DISTRO_FEDORA)
    } else if source.contains("alpine") {
        Some(icons::DISTRO_ALPINE)
    } else if source.contains("arch linux") || source.contains("archlinux") {
        Some(icons::DISTRO_ARCH)
    } else if source.contains("red hat")
        || source.contains("redhat")
        || source.contains("rhel")
        || source.contains("alma")
        || source.contains("oracle linux")
    {
        Some(icons::DISTRO_REDHAT)
    } else if source.contains("opensuse") || source.contains("suse") {
        Some(icons::DISTRO_OPENSUSE)
    } else {
        None
    }
}

/// 渲染字母头像（用于自定义供应商：圆底 + 名称首字符）
/// 颜色按字符确定性选取，深浅主题下均可读。
pub fn render_letter_avatar(ch: char, size: f32) -> impl IntoElement {
    // 一组柔和的强调色（圆底用低透明度，文字用实色）
    const PALETTE: [u32; 6] = [
        0x3b82f6, // 蓝
        0x22c55e, // 绿
        0xf59e0b, // 橙
        0xa855f7, // 紫
        0xef4444, // 红
        0x14b8a6, // 青
    ];
    let idx = (ch as usize) % PALETTE.len();
    let color: Hsla = gpui::rgb(PALETTE[idx]).into();
    let font_px = (size * 0.5).max(9.);

    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(color.opacity(0.18))
        .child(
            div()
                .text_color(color)
                .text_size(px(font_px))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(ch.to_string())),
        )
}
