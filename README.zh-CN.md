# ShellMaster3

一个使用 Rust 构建的现代化桌面 SSH/SFTP 客户端。

[English README](README.md)

## 项目简介

ShellMaster3 是一个面向远程服务器管理的桌面应用，当前聚焦以下核心能力：

- SSH 终端会话
- SFTP 文件浏览与传输
- 远程系统监控
- 快捷命令与已知主机管理
- 可配置的界面与连接设置

项目仍在持续演进，优先完善核心场景和架构稳定性。

## AI 声明

本项目采用 vibe coding，代码由 AI 生成。

## 功能特性

- SSH 连接管理
- 密码认证与私钥认证
- 跳板机与代理支持
- Host Key 信任流程与 Known Hosts 持久化
- 终端标签/会话界面
- SFTP 文件列表、目录树、上传下载与基础远程编辑流程
- 主机监控面板（CPU、内存、磁盘、网络）
- 服务器、设置、快捷命令、已知主机的本地持久化
- 内置中英文界面支持

## 技术栈

- 语言：Rust（Edition 2021）
- UI：`gpui`、`gpui-component`
- SSH：`russh`
- SFTP：`russh-sftp`
- 终端模拟：`alacritty_terminal`
- 异步运行时：`tokio`
- 序列化：`serde`、`serde_json`

## 项目结构

```text
src/
  components/        # 通用 UI 组件（terminal/sftp/monitor/dialog）
  models/            # 领域模型与设置模型
  pages/             # 页面（home、connecting、session）
  services/          # 存储、SSH/SFTP 服务、监控服务
  ssh/               # SSH 客户端/会话/连接器/重连逻辑
  state/             # 全局与会话状态
  terminal/          # 终端桥接、渲染、按键处理

docs/
  architecture.md
  ssh-connection-architecture.md
  sftp-architecture.md
  monitor-architecture.md
  settings-design.md
```

## 环境要求

- Rust 稳定版工具链
- Cargo
- `gpui` 支持的桌面运行环境

## 构建与运行

```bash
cargo build
cargo run
```

开启调试日志：

```bash
RUST_LOG=debug cargo run
```

## 配置与数据文件

ShellMaster3 会在系统配置目录下创建 `shellmaster` 目录。

- macOS：`~/Library/Application Support/shellmaster`
- Linux：`~/.config/shellmaster`
- Windows：`C:\Users\<用户名>\AppData\Roaming\shellmaster`

关键文件/目录：

- `servers.json`
- `settings.json`
- `snippets.json`
- `known_hosts.json`
- `keys/`（应用托管的私钥目录）

## 安全说明

- 导入的私钥会复制到 `keys/` 目录中统一管理。
- 在 Unix 系统下，目录/文件权限会收紧（目录 `0700`，文件 `0600`）。
- 启动时会自动迁移旧版私钥路径配置。

详见：`PRIVATE_KEY_MIGRATION.md`

## TODO / Roadmap

### 进行中

- [ ] 终端体验打磨（快捷键与行为一致性）
- [ ] SFTP 编辑与传输体验加固
- [ ] 监控数据刷新与性能优化

### 计划中

- [ ] 跨平台打包与发布流程
- [ ] 支持基于 WebDAV 的多端配置文件同步
- [ ] 跳板机连接能力增强
- [ ] 扩展快捷键能力与自定义选项
- [ ] 更好的连接诊断与错误提示
- [ ] 同步/导入导出流程优化
- [ ] 更完善的自动化测试（单元 + 集成）

## 贡献指南

欢迎贡献代码与建议。

建议流程：

1. Fork 仓库并创建功能分支。
2. 保持改动聚焦并附带必要说明。
3. 提交 PR 前先完成本地检查。
4. 若涉及架构变更，请关联相应设计文档。

## 许可证

本项目使用 MIT 许可证，详见 `LICENSE`。
