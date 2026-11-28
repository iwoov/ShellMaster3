// 客户端关键字高亮：按空白切词，把常见的错误/警告/成功词、IP、URL、路径、提示符
// 映射到 ANSI 调色板索引。纯函数，终端渲染与设置预览共用同一套规则。

use std::ops::Range;

const RED: u8 = 1;
const GREEN: u8 = 2;
const YELLOW: u8 = 3;
const BLUE: u8 = 4;
const CYAN: u8 = 6;

const ERROR_WORDS: &[&str] = &[
    "error", "errors", "fail", "failed", "failure", "fatal", "denied", "critical", "panic",
    "panicked",
];
const WARN_WORDS: &[&str] = &["warn", "warning", "warnings", "deprecated"];
const OK_WORDS: &[&str] = &[
    "ok", "success", "done", "active", "running", "enabled", "passed",
];

/// 词首尾会被剥掉的标点
fn is_edge_punct(c: char) -> bool {
    matches!(c, ',' | ';' | ':' | '(' | ')' | '[' | ']' | '"' | '\'')
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// 对一行文本分类，返回「字符下标范围 → ANSI 调色板索引」。
/// 范围以 `char` 为单位（不是字节），互不重叠，按出现顺序排列。
pub fn classify_line(line: &str) -> Vec<(Range<usize>, u8)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(Range<usize>, u8)> = Vec::new();

    // 切词：连续非空白字符
    let mut tokens: Vec<Range<usize>> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        tokens.push(start..i);
    }

    let mut t = 0;
    while t < tokens.len() {
        let tok = tokens[t].clone();

        // 行首 user@host 提示符
        if t == 0 {
            if let Some(mut parts) = classify_prompt(&chars, tok.clone()) {
                out.append(&mut parts);
                t += 1;
                continue;
            }
        }

        let core = strip_edges(&chars, tok.clone());
        if core.is_empty() {
            t += 1;
            continue;
        }
        let word: String = chars[core.clone()].iter().collect();
        let lower = word.to_ascii_lowercase();

        // "not found" 跨两个词
        if lower == "not" && t + 1 < tokens.len() {
            let next = strip_edges(&chars, tokens[t + 1].clone());
            let next_word: String = chars[next.clone()].iter().collect();
            if next_word.eq_ignore_ascii_case("found") {
                out.push((core.start..next.end, RED));
                t += 2;
                continue;
            }
        }

        if let Some(slot) = classify_word(&word, &lower) {
            out.push((core, slot));
        }
        t += 1;
    }

    out
}

/// 剥掉词首尾的标点，返回剩余部分的字符范围
fn strip_edges(chars: &[char], range: Range<usize>) -> Range<usize> {
    let mut start = range.start;
    let mut end = range.end;
    while start < end && is_edge_punct(chars[start]) {
        start += 1;
    }
    while end > start && is_edge_punct(chars[end - 1]) {
        end -= 1;
    }
    start..end
}

fn classify_word(word: &str, lower: &str) -> Option<u8> {
    if is_ipv4(word) || lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(CYAN);
    }
    if word.starts_with('/')
        || word.starts_with("~/")
        || word.starts_with("./")
        || word.ends_with('/')
    {
        return Some(BLUE);
    }
    if ERROR_WORDS.contains(&lower) {
        Some(RED)
    } else if WARN_WORDS.contains(&lower) {
        Some(YELLOW)
    } else if OK_WORDS.contains(&lower) {
        Some(GREEN)
    } else {
        None
    }
}

/// `a.b.c.d`，可带 `:端口` 或 `/掩码`
fn is_ipv4(word: &str) -> bool {
    let end = word
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(word.len());
    let (addr, rest) = word.split_at(end);
    if !rest.is_empty() {
        let digits = match rest.strip_prefix(':').or_else(|| rest.strip_prefix('/')) {
            Some(d) => d,
            None => return false,
        };
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    let parts: Vec<&str> = addr.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.len() <= 3 && p.parse::<u16>().map_or(false, |n| n <= 255)
        })
}

/// 解析 `user@host`、`user@host:path#` 形式的提示符词。
fn classify_prompt(chars: &[char], tok: Range<usize>) -> Option<Vec<(Range<usize>, u8)>> {
    let mut i = tok.start;
    let user_start = i;
    while i < tok.end && is_name_char(chars[i]) {
        i += 1;
    }
    if i == user_start || i >= tok.end || chars[i] != '@' {
        return None;
    }
    i += 1;
    let host_start = i;
    while i < tok.end && is_name_char(chars[i]) {
        i += 1;
    }
    if i == host_start {
        return None;
    }
    let mut out = vec![(tok.start..i, GREEN)];
    if i == tok.end {
        return Some(out);
    }
    if chars[i] != ':' {
        return None;
    }
    // 冒号之后是路径，末尾可能跟 `#` / `$` / `%`
    let path_start = i + 1;
    let mut path_end = tok.end;
    while path_end > path_start && matches!(chars[path_end - 1], '#' | '$' | '%') {
        path_end -= 1;
    }
    if path_end > path_start && matches!(chars[path_start], '/' | '~') {
        out.push((path_start..path_end, BLUE));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取出某范围对应的文本，便于断言
    fn spans(line: &str) -> Vec<(String, u8)> {
        let chars: Vec<char> = line.chars().collect();
        classify_line(line)
            .into_iter()
            .map(|(r, c)| (chars[r].iter().collect(), c))
            .collect()
    }

    fn s(text: &str, slot: u8) -> (String, u8) {
        (text.to_string(), slot)
    }

    #[test]
    fn error_keyword() {
        assert_eq!(spans("ERROR: disk full"), vec![s("ERROR", 1)]);
        assert_eq!(spans("Permission denied"), vec![s("denied", 1)]);
        assert_eq!(spans("bash: foo: command not found"), vec![s("not found", 1)]);
    }

    #[test]
    fn warn_and_ok_keywords() {
        assert_eq!(spans("warning: deprecated"), vec![s("warning", 3), s("deprecated", 3)]);
        assert_eq!(spans("Active: active (running)"), vec![s("Active", 2), s("active", 2), s("running", 2)]);
        assert_eq!(spans("[ OK ]"), vec![s("OK", 2)]);
    }

    #[test]
    fn ip_and_url() {
        assert_eq!(spans("64 bytes from 8.8.8.8: ttl=5"), vec![s("8.8.8.8", 6)]);
        assert_eq!(spans("listen 10.0.0.1:22"), vec![s("10.0.0.1:22", 6)]);
        assert_eq!(spans("see https://example.com/a)"), vec![s("https://example.com/a", 6)]);
        assert!(spans("version 1.2.3").is_empty());
        assert!(spans("999.1.1.1").is_empty());
    }

    #[test]
    fn paths() {
        assert_eq!(spans("cat /etc/hosts ~/x ./run"), vec![s("/etc/hosts", 4), s("~/x", 4), s("./run", 4)]);
        assert_eq!(spans("docs/  src/  a.txt"), vec![s("docs/", 4), s("src/", 4)]);
        assert_eq!(spans("open '/tmp/a b'"), vec![s("/tmp/a", 4)]);
    }

    #[test]
    fn prompt() {
        assert_eq!(
            spans("root@host:~# ls /etc"),
            vec![s("root@host", 2), s("~", 4), s("/etc", 4)]
        );
        assert_eq!(
            spans("user@srv:/var/log$ x"),
            vec![s("user@srv", 2), s("/var/log", 4)]
        );
        assert_eq!(spans("root@host # x"), vec![s("root@host", 2)]);
    }

    #[test]
    fn no_substring_match() {
        assert!(spans("okay broker failover undone").is_empty());
        assert!(spans("terror errorless").is_empty());
    }

    #[test]
    fn char_indices_not_bytes() {
        // 中文字符占多个字节，范围必须按字符计
        assert_eq!(classify_line("你好 error"), vec![(3..8, 1)]);
    }
}
