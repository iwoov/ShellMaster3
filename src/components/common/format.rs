// 数值格式化：字节数、速度、运行时长。监控、SFTP 和传输列表共用，保证同一个数在各处显示一致。

use crate::i18n;
use crate::models::settings::Language;

const KB: f64 = 1024.;
const MB: f64 = KB * 1024.;
const GB: f64 = MB * 1024.;
const TB: f64 = GB * 1024.;

/// 字节数：1024 进制，保留一位小数，不足 1 KB 时显示整数字节。例：`512 B`、`1.5 KB`、`1.2 GB`
pub fn format_bytes(bytes: u64) -> String {
    let value = bytes as f64;
    if value >= TB {
        format!("{:.1} TB", value / TB)
    } else if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// 速度：单位与 [`format_bytes`] 相同，后接 `/s`。负数和非数值按 0 处理
pub fn format_speed(bytes_per_sec: f64) -> String {
    let bytes = if bytes_per_sec.is_finite() && bytes_per_sec > 0. {
        bytes_per_sec.round() as u64
    } else {
        0
    };
    format!("{}/s", format_bytes(bytes))
}

/// 「已用 / 总量」，两边用同一个单位，例如 `3.1 / 7.8 GB`、`512 / 1024 MB`
pub fn format_usage(used: u64, total: u64) -> String {
    let (divisor, unit) = unit_for(total.max(used));
    if divisor == 1. {
        return format!("{} / {} {}", used, total, unit);
    }
    format!(
        "{:.1} / {:.1} {}",
        used as f64 / divisor,
        total as f64 / divisor,
        unit
    )
}

fn unit_for(bytes: u64) -> (f64, &'static str) {
    let value = bytes as f64;
    if value >= TB {
        (TB, "TB")
    } else if value >= GB {
        (GB, "GB")
    } else if value >= MB {
        (MB, "MB")
    } else if value >= KB {
        (KB, "KB")
    } else {
        (1., "B")
    }
}

/// 运行时长：「45天 5小时 12分钟」/「45d 5h 12m」，不足一分钟显示「0分钟」
pub fn format_uptime(seconds: u64, lang: &Language) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    let day = i18n::t(lang, "monitor.detail.uptime_days");
    let hour = i18n::t(lang, "monitor.detail.uptime_hours");
    let minute = i18n::t(lang, "monitor.detail.uptime_minutes");

    if days > 0 {
        format!("{days}{day} {hours}{hour} {minutes}{minute}")
    } else if hours > 0 {
        format!("{hours}{hour} {minutes}{minute}")
    } else {
        format!("{minutes}{minute}")
    }
}

/// 百分比，保留一位小数，例如 `23.4%`
pub fn format_percent(percent: f32) -> String {
    let percent = if percent.is_finite() { percent } else { 0. };
    format!("{:.1}%", percent.clamp(0., 100.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units_with_one_decimal() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(1_288_490_189), "1.2 GB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024 * 1024), "3.0 TB");
    }

    #[test]
    fn speed_rounds_and_ignores_invalid_values() {
        assert_eq!(format_speed(0.), "0 B/s");
        assert_eq!(format_speed(-5.), "0 B/s");
        assert_eq!(format_speed(f64::NAN), "0 B/s");
        assert_eq!(format_speed(511.6), "512 B/s");
        assert_eq!(format_speed(2.5 * 1024. * 1024.), "2.5 MB/s");
    }

    #[test]
    fn usage_shares_the_unit_of_the_larger_value() {
        assert_eq!(format_usage(3_328_599_654, 8_375_186_227), "3.1 / 7.8 GB");
        assert_eq!(format_usage(512 * 1024, 2 * 1024 * 1024), "0.5 / 2.0 MB");
        assert_eq!(format_usage(10, 900), "10 / 900 B");
        assert_eq!(format_usage(0, 0), "0 / 0 B");
    }

    #[test]
    fn uptime_drops_leading_zero_units() {
        let zh = Language::Chinese;
        let en = Language::English;
        assert_eq!(format_uptime(59, &zh), "0分钟");
        assert_eq!(format_uptime(3 * 3600 + 5 * 60, &zh), "3小时 5分钟");
        assert_eq!(
            format_uptime(45 * 86_400 + 5 * 3600 + 12 * 60, &zh),
            "45天 5小时 12分钟"
        );
        assert_eq!(format_uptime(2 * 86_400 + 60, &en), "2d 0h 1m");
    }

    #[test]
    fn percent_is_clamped_to_the_valid_range() {
        assert_eq!(format_percent(23.44), "23.4%");
        assert_eq!(format_percent(-1.), "0.0%");
        assert_eq!(format_percent(140.), "100.0%");
        assert_eq!(format_percent(f32::NAN), "0.0%");
    }
}
