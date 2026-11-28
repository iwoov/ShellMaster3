// 工具 2：run_in_terminal —— 在当前已打开的可见 SSH 终端里直接输入并执行命令

use std::time::{Duration, Instant};

use rig_core::completion::ToolAnnotations;
use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::terminal::read_term_snapshot;

use super::{truncate_output, AgentSessionContext, AgentToolError};

const MAX_OUTPUT_BYTES: usize = 12 * 1024;
const POLL_INTERVAL_MS: u64 = 200;
const SETTLE_AFTER_CHANGE_MS: u64 = 1200;
const SETTLE_NO_CHANGE_MS: u64 = 2200;

#[derive(Clone)]
pub struct RunInTerminalTool {
    ctx: AgentSessionContext,
}

impl RunInTerminalTool {
    pub fn new(ctx: AgentSessionContext) -> Self {
        Self { ctx }
    }
}

#[derive(Debug, Deserialize)]
pub struct RunInTerminalArgs {
    /// 要在当前终端执行的命令（无需附带末尾换行符）
    pub command: String,
    /// 等待输出稳定的最长时间（秒），默认 8 秒，最大 30 秒
    #[serde(default)]
    pub wait_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct RunInTerminalOutput {
    /// "settled" | "still_running" | "blocked_alt_screen"
    pub status: String,
    pub output: String,
    pub alt_screen: bool,
    pub truncated: bool,
}

impl Tool for RunInTerminalTool {
    const NAME: &'static str = "run_in_terminal";

    type Args = RunInTerminalArgs;
    type Output = RunInTerminalOutput;
    type Error = AgentToolError;

    fn description(&self) -> String {
        "Type and execute a command directly inside the user's currently open interactive SSH terminal so the user can see it live and preserve the shell's working directory/environment. Waits for output to settle (up to wait_secs) and returns the newly produced screen output.".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to run in the active terminal (trailing newline is added automatically)."
                },
                "wait_secs": {
                    "type": "integer",
                    "description": "Max seconds to wait for terminal output to settle (default 8, max 30).",
                    "minimum": 1,
                    "maximum": 30
                }
            },
            "required": ["command"],
            "additionalProperties": false
        })
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            title: Some("Run In Active Terminal".to_string()),
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            idempotent_hint: Some(false),
            open_world_hint: Some(true),
        }
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let cmd = args.command.trim_end_matches(['\r', '\n']);
        if cmd.trim().is_empty() {
            return Err(AgentToolError::InvalidArgs("命令不能为空".to_string()));
        }

        let Some(pty) = &self.ctx.active_pty else {
            return Err(AgentToolError::NoActiveTerminal);
        };
        let Some(term) = &self.ctx.active_term else {
            return Err(AgentToolError::NoActiveTerminal);
        };

        let before = read_term_snapshot(term, 160, true);
        if before.is_alt_screen {
            return Ok(RunInTerminalOutput {
                status: "blocked_alt_screen".to_string(),
                output: "当前终端正处于全屏交互程序（alt_screen，如 vim/top/less）中，已阻止直接写入 shell 命令。请先用 read_terminal_screen 查看界面，或改用 ssh_exec / send_terminal_keys。".to_string(),
                alt_screen: true,
                truncated: false,
            });
        }

        let mut payload = cmd.as_bytes().to_vec();
        payload.push(b'\r');
        pty.queue_write(payload)
            .map_err(|e| AgentToolError::Ssh(e.to_string()))?;

        let max_wait = args
            .wait_secs
            .unwrap_or(self.ctx.command_timeout_secs.min(8).max(3))
            .clamp(1, 30);
        let deadline = Instant::now() + Duration::from_secs(max_wait);
        let started = Instant::now();
        let mut last_text = before.text.clone();
        let mut last_change = started;
        let mut ever_changed = false;
        let mut settled = false;

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let sleep_dur = Duration::from_millis(POLL_INTERVAL_MS).min(deadline - now);
            tokio::select! {
                _ = self.ctx.cancel_token.cancelled() => {
                    return Err(AgentToolError::Cancelled);
                }
                _ = tokio::time::sleep(sleep_dur) => {}
            }

            let cur = read_term_snapshot(term, 160, true);
            if cur.text != last_text {
                last_text = cur.text;
                last_change = Instant::now();
                ever_changed = true;
            } else {
                let quiet_ms = last_change.elapsed().as_millis() as u64;
                let elapsed_ms = started.elapsed().as_millis() as u64;
                if (ever_changed && quiet_ms >= SETTLE_AFTER_CHANGE_MS && elapsed_ms >= 500)
                    || (!ever_changed && elapsed_ms >= SETTLE_NO_CHANGE_MS)
                {
                    settled = true;
                    break;
                }
            }
        }

        let after = read_term_snapshot(term, 160, true);
        let delta = extract_new_terminal_output(&before.text, &after.text);
        let (output, truncated) = truncate_output(&delta, MAX_OUTPUT_BYTES);

        Ok(RunInTerminalOutput {
            status: if settled {
                "settled".to_string()
            } else {
                "still_running".to_string()
            },
            output,
            alt_screen: after.is_alt_screen,
            truncated,
        })
    }
}

/// 从执行前/后两份终端文本快照中提取新增部分；若屏幕发生清屏或大幅滚屏，则回退返回最新尾部 80 行。
fn extract_new_terminal_output(before: &str, after: &str) -> String {
    if before.is_empty() {
        return after.to_string();
    }
    if let Some(rest) = after.strip_prefix(before) {
        let trimmed = rest.trim_start_matches('\n');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    // 查找 before 末尾若干行在 after 中的锚点，提取其后新增内容
    let anchor_len = before_lines.len().min(6);
    if anchor_len > 0 && after_lines.len() >= anchor_len {
        let anchor = &before_lines[before_lines.len() - anchor_len..];
        for start in (0..=after_lines.len() - anchor_len).rev() {
            if &after_lines[start..start + anchor_len] == anchor {
                let tail = &after_lines[start + anchor_len..];
                if !tail.is_empty() {
                    return tail.join("\n");
                }
                break;
            }
        }
    }

    // 兜底：返回最新最多 80 行
    let take_from = after_lines.len().saturating_sub(80);
    after_lines[take_from..].join("\n")
}
