// 工具 4：ssh_exec —— 通过独立 SSH exec 通道后台静默执行命令（干净 stdout/stderr + exit_code）

use std::time::Duration;

use rig_core::completion::ToolAnnotations;
use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{truncate_output, AgentSessionContext, AgentToolError};

const MAX_STDOUT_BYTES: usize = 16 * 1024;
const MAX_STDERR_BYTES: usize = 6 * 1024;

#[derive(Clone)]
pub struct SshExecTool {
    ctx: AgentSessionContext,
}

impl SshExecTool {
    pub fn new(ctx: AgentSessionContext) -> Self {
        Self { ctx }
    }
}

#[derive(Debug, Deserialize)]
pub struct SshExecArgs {
    /// 要执行的远端命令
    pub command: String,
    /// 超时秒数（可选，默认采用 Agent 设置里的 command_timeout_secs，上限 60 秒）
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SshExecOutput {
    pub exit_code: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

impl Tool for SshExecTool {
    const NAME: &'static str = "ssh_exec";

    type Args = SshExecArgs;
    type Output = SshExecOutput;
    type Error = AgentToolError;

    fn description(&self) -> String {
        "Execute a non-interactive command silently on the remote SSH server over an independent exec channel, returning clean stdout, stderr, and exit_code without disturbing the user's open terminal. Prefer this tool for read-only inspection, diagnostics, and log queries.".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute on the remote server."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default 15, max 60).",
                    "minimum": 1,
                    "maximum": 60
                }
            },
            "required": ["command"],
            "additionalProperties": false
        })
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            title: Some("SSH Exec".to_string()),
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
        let cmd = args.command.trim();
        if cmd.is_empty() {
            return Err(AgentToolError::InvalidArgs("命令不能为空".to_string()));
        }
        let Some(session) = &self.ctx.ssh_session else {
            return Err(AgentToolError::NotConnected);
        };
        if !session.is_alive() {
            return Err(AgentToolError::NotConnected);
        }

        let timeout_secs = args
            .timeout_secs
            .unwrap_or(self.ctx.command_timeout_secs)
            .clamp(1, 60);

        let channel = session
            .open_exec()
            .await
            .map_err(|e| AgentToolError::Ssh(e.to_string()))?;

        let exec_fut = channel.exec(cmd);
        let result = tokio::select! {
            _ = self.ctx.cancel_token.cancelled() => {
                return Err(AgentToolError::Cancelled);
            }
            res = tokio::time::timeout(Duration::from_secs(timeout_secs), exec_fut) => {
                match res {
                    Ok(inner) => inner.map_err(|e| AgentToolError::Ssh(e.to_string()))?,
                    Err(_) => return Err(AgentToolError::Timeout(timeout_secs)),
                }
            }
        };

        let (stdout, out_trunc) = truncate_output(&result.stdout_string(), MAX_STDOUT_BYTES);
        let (stderr, err_trunc) = truncate_output(&result.stderr_string(), MAX_STDERR_BYTES);

        Ok(SshExecOutput {
            exit_code: result.exit_code,
            stdout,
            stderr,
            truncated: out_trunc || err_trunc,
        })
    }
}
