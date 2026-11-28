// Agent System Prompt 组装器：内置终端工具协议 + 用户自定义 System Prompt + 会话环境上下文

/// 构建最终发给 Agent 的 System Prompt（preamble）。
pub fn build_agent_system_prompt(
    user_system_prompt: &str,
    terminal_context: Option<&str>,
    has_pty_tools: bool,
    has_exec_tool: bool,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // 1. 内置 Agent 运行时规则（确保模型正确、安全地使用终端工具）
    let mut tool_guide = String::from(
        "你运行在 ShellMaster3 SSH 终端客户端内部，作为内置终端 Agent 直接辅助用户排查与操作远端服务器。\n\n\
## 可用工具与使用规范\n\
- `read_terminal_screen`：随时读取用户当前已打开终端的屏幕内容与最近历史，同时返回是否处于全屏 TUI 程序（如 vim/top/less）。当你需要了解终端现状、报错信息或交互提示时，先调用此工具。\n",
    );
    if has_exec_tool {
        tool_guide.push_str(
            "- `ssh_exec`：通过独立 SSH exec 通道在后台静默执行命令，返回干净的 stdout/stderr 和退出码，不会干扰用户当前终端画面。**优先用于只读信息采集、日志查看、状态诊断**（如 `df -h`、`systemctl status`、`journalctl`、`cat` 等）。\n",
        );
    }
    if has_pty_tools {
        tool_guide.push_str(
            "- `run_in_terminal`：直接在用户当前可见的交互终端里输入并执行命令。适用于需要复用当前 shell 的工作目录/环境变量/虚拟环境，或用户明确要求在当前终端执行的任务。如果终端处于全屏 TUI（alt_screen=true），禁止盲目输入 shell 命令。\n\
- `send_terminal_keys`：向当前可见终端发送按键或控制字符（支持 `<Ctrl-C>`、`<Ctrl-D>`、`<Enter>`、`<Esc>`、`<Tab>` 等），用于中断卡住的进程或响应交互式提示。\n",
        );
    }
    tool_guide.push_str(
        "\n## 安全与交互原则\n\
1. 先观察再动手：对服务器做任何修改前，先用只读方式确认现状。\n\
2. 尊重用户审批：部分高风险或写操作会弹出审批请求，如果用户拒绝了某次工具调用，不要反复重试同一命令，直接尊重用户决定并询问替代方案。\n\
3. 严禁输出臆造的命令结果：所有服务器状态必须来自工具真实返回或终端上下文。\n\
4. 如果命令超时且仍在运行（例如等待输入密码或 `[Y/n]`），直接查看屏幕结果并如实告知用户或按需处理。",
    );
    parts.push(tool_guide);

    // 2. 用户自定义系统提示词
    let custom = user_system_prompt.trim();
    if !custom.is_empty() {
        parts.push(format!("## 用户自定义指令\n{custom}"));
    }

    // 3. 动态终端环境上下文（主机/用户/操作系统等）
    if let Some(ctx) = terminal_context {
        let trimmed = ctx.trim();
        if !trimmed.is_empty() {
            parts.push(format!("## 当前 SSH 会话上下文\n{trimmed}"));
        }
    }

    parts.join("\n\n")
}
