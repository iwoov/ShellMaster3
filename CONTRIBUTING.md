# Contributing to ShellMaster3

Thank you for your interest in contributing to **ShellMaster3**!

## Prerequisites

- **Rust** (stable toolchain, edition 2021) — install via [rustup](https://rustup.rs/)
- **Platform dependencies** for GPUI:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Linux**: `libasound2-dev`, `libxkbcommon-dev`, `libwayland-dev`, `libvulkan-dev`, `libxcb1-dev`, `pkg-config`
  - **Windows**: Visual Studio Build Tools with C++ workload (MSVC)

## Getting Started

```bash
git clone https://github.com/iwoov/ShellMaster3.git
cd ShellMaster3

# Run the app in debug mode
cargo run

# Enable verbose application logs while debugging
RUST_LOG=shellmaster3=debug cargo run
```

## Code Checks & Tests

Before opening a pull request, make sure your changes compile cleanly without warnings and pass the full test suite:

```bash
cargo test --no-fail-fast
```

## Project Layout

```text
src/
├── main.rs            # Application entry point and window bootstrap
├── models/            # Server, SFTP, Monitor, AI, and Settings data models
├── ssh/               # SSH client, auth, jump host, proxy, session, and port forwarding
├── terminal/          # Alacritty terminal emulator bridge, input, highlight, and URL detection
├── services/          # Storage, AES-256-GCM crypto, SFTP service, Monitor collector, AI agent
├── state/             # GPUI application and session state management
├── pages/             # Home page and Session workspace views
├── components/        # Reusable UI dialogs, panels, SFTP widgets, and Monitor cards
├── theme/             # Light / dark palettes and typography
└── i18n.rs            # Chinese / English localization strings
```

## Pull Request Guidelines

- Keep PRs focused on a single bug fix or feature.
- Add unit tests for new parsing, protocol, crypto, or state logic when practical.
- Never commit personal credentials, private keys, hostnames, or local machine paths in fixtures or comments.
- Update both `README.md` / `README.zh-CN.md` and `src/i18n.rs` when adding user-facing features or UI text.
- By submitting a contribution, you agree that it will be licensed under the repository's [MIT License](LICENSE).
