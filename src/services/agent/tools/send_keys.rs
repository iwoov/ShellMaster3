// 工具 3：send_terminal_keys —— 向当前可见终端发送按键或控制字符（Ctrl-C / Enter / 交互响应等）

use std::time::Duration;

use rig_core::completion::ToolAnnotations;
use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::terminal::read_term_snapshot;

use super::{truncate_output, AgentSessionContext, AgentToolError};

const MAX_OUTPUT_BYTES: usize = 8 * 1024;

#[derive(Clone)]
pub struct SendTerminalKeysTool {
    ctx: AgentSessionContext,
}

impl SendTerminalKeysTool {
    pub fn new(ctx: AgentSessionContext) -> Self {
        Self { ctx }
    }
}

#[derive(Debug, Deserialize)]
pub struct SendTerminalKeysArgs {
    /// 要发送的按键序列，支持普通字符和特殊记号：<Ctrl-C>, <Ctrl-D>, <Ctrl-Z>, <Ctrl-L>, <Enter>, <Esc>, <Tab>, <Up>, <Down>, <Left>, <Right>
    pub keys: String,
    /// 发送后等待刷新屏幕的毫秒数（默认 800，最大 5000）
    #[serde(default)]
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SendTerminalKeysOutput {
    pub bytes_sent: usize,
    pub screen_tail: String,
    pub alt_screen: bool,
    pub truncated: bool,
}

impl Tool for SendTerminalKeysTool {
    const NAME: &'static str = "send_terminal_keys";

    type Args = SendTerminalKeysArgs;
    type Output = SendTerminalKeysOutput;
    type Error = AgentToolError;

    fn description(&self) -> String {
        "Send raw keystrokes or control keys to the active interactive SSH terminal (useful for interrupting hung commands or answering interactive prompts). Supported special tokens: <Ctrl-C>, <Ctrl-D>, <Ctrl-Z>, <Ctrl-L>, <Enter>, <Esc>, <Tab>, <Backspace>, <Up>, <Down>, <Left>, <Right>.".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "keys": {
                    "type": "string",
                    "description": "Text and/or special key tokens to send, e.g. '<Ctrl-C>' or 'y<Enter>'."
                },
                "wait_ms": {
                    "type": "integer",
                    "description": "Milliseconds to wait before reading updated terminal screen (default 800, max 5000).",
                    "minimum": 100,
                    "maximum": 5000
                }
            },
            "required": ["keys"],
            "additionalProperties": false
        })
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            title: Some("Send Terminal Keys".to_string()),
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(false),
            open_world_hint: Some(true),
        }
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        if args.keys.is_empty() {
            return Err(AgentToolError::InvalidArgs("keys 不能为空".to_string()));
        }
        let Some(pty) = &self.ctx.active_pty else {
            return Err(AgentToolError::NoActiveTerminal);
        };
        let Some(term) = &self.ctx.active_term else {
            return Err(AgentToolError::NoActiveTerminal);
        };

        let bytes = encode_key_sequence(&args.keys);
        let bytes_sent = bytes.len();
        pty.queue_write(bytes)
            .map_err(|e| AgentToolError::Ssh(e.to_string()))?;

        let wait = Duration::from_millis(args.wait_ms.unwrap_or(800).clamp(100, 5000));
        tokio::select! {
            _ = self.ctx.cancel_token.cancelled() => {
                return Err(AgentToolError::Cancelled);
            }
            _ = tokio::time::sleep(wait) => {}
        }

        let snap = read_term_snapshot(term, 60, false);
        let (screen_tail, truncated) = truncate_output(&snap.text, MAX_OUTPUT_BYTES);
        Ok(SendTerminalKeysOutput {
            bytes_sent,
            screen_tail,
            alt_screen: snap.is_alt_screen,
            truncated,
        })
    }
}

/// 把包含 `<Ctrl-C>` / `<Enter>` 等记号的按键字符串编码为终端字节序列。
pub fn encode_key_sequence(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        if rest.starts_with('<') {
            if let Some(close) = rest.find('>') {
                let token = &rest[1..close];
                if let Some(seq) = map_special_token(token) {
                    out.extend_from_slice(seq);
                    rest = &rest[close + 1..];
                    continue;
                }
            }
        }
        if let Some(ch) = rest.chars().next() {
            if ch == '\n' {
                out.push(b'\r');
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            rest = &rest[ch.len_utf8()..];
        }
    }

    out
}

fn map_special_token(token: &str) -> Option<&'static [u8]> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl-c" | "c-c" => Some(b"\x03"),
        "ctrl-d" | "c-d" => Some(b"\x04"),
        "ctrl-z" | "c-z" => Some(b"\x1a"),
        "ctrl-l" | "c-l" => Some(b"\x0c"),
        "ctrl-a" | "c-a" => Some(b"\x01"),
        "ctrl-e" | "c-e" => Some(b"\x05"),
        "ctrl-u" | "c-u" => Some(b"\x15"),
        "enter" | "return" | "cr" => Some(b"\r"),
        "esc" | "escape" => Some(b"\x1b"),
        "tab" => Some(b"\t"),
        "backspace" | "bs" => Some(b"\x7f"),
        "up" => Some(b"\x1b[A"),
        "down" => Some(b"\x1b[B"),
        "right" => Some(b"\x1b[C"),
        "left" => Some(b"\x1b[D"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_mixed_keys() {
        assert_eq!(encode_key_sequence("<Ctrl-C>"), b"\x03");
        assert_eq!(encode_key_sequence("y<Enter>"), b"y\r");
        assert_eq!(encode_key_sequence("<Esc>:q!<Enter>"), b"\x1b:q!\r");
        assert_eq!(encode_key_sequence("<unknown>"), b"<unknown>");
    }
}
