# Security Policy

## Supported Versions

Security fixes are applied to the latest release on the `main` branch.

| Version | Supported |
| ------- | --------- |
| Latest (`main` / latest release) | Yes |
| Older releases | No |

## Reporting a Vulnerability

Because ShellMaster3 handles SSH connections, SFTP file transfers, port forwarding tunnels, and locally encrypted credentials, please **do not** disclose security vulnerabilities in public GitHub issues or pull requests.

Instead, report vulnerabilities privately through GitHub Security Advisories:

- https://github.com/iwoov/ShellMaster3/security/advisories/new

Please include:

1. A clear description of the issue and potential impact.
2. Reproduction steps or a minimal proof of concept.
3. Affected OS, architecture, and ShellMaster3 version.

### What to Expect

- We aim to acknowledge receipt within **72 hours**.
- After verifying the issue, we will work on a fix and coordinate disclosure before publishing a release.

## Scope

Security reports are especially welcome for:

- SSH host key verification, authentication (`password`, `keyboard-interactive`, private key, `ssh-agent`), or proxy/jump-host handling
- SFTP path handling, recursive deletion, or permission operations
- Port forwarding (`-L`, `-R`, `-D` SOCKS5) isolation and lifecycle cleanup
- At-rest encryption (`AES-256-GCM`) and local storage permissions for saved credentials and AI API keys
- Terminal escape sequence handling (e.g., OSC 52 clipboard policy, bracketed paste, URL click handling)
