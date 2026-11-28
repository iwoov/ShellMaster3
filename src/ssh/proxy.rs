// 代理连接模块
// 支持 HTTP CONNECT、SOCKS4(a) 和 SOCKS5 代理

use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;

use super::config::{ProxyConfig, ProxyType};
use super::error::SshError;
use super::net::connect_tcp;

/// 通过代理连接到目标主机，整体耗时不超过 `limit`
///
/// 目标主机名直接交给代理解析（SOCKS4a / SOCKS5 域名方式），本地不做 DNS 查询。
pub async fn connect_via_proxy(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
    limit: Duration,
) -> Result<TcpStream, SshError> {
    let kind = proxy.proxy_type.label();
    if proxy.host.trim().is_empty() || proxy.port == 0 {
        return Err(SshError::Config(format!(
            "{} proxy address is not configured",
            kind
        )));
    }

    timeout(limit, async {
        let stream = connect_tcp(&proxy.host, proxy.port, limit)
            .await
            .map_err(|e| {
                SshError::Proxy(format!(
                    "failed to connect to {} proxy {}:{}: {}",
                    kind, proxy.host, proxy.port, e
                ))
            })?;

        match proxy.proxy_type {
            ProxyType::Socks5 => connect_socks5(stream, proxy, target_host, target_port).await,
            ProxyType::Socks4 => connect_socks4(stream, proxy, target_host, target_port).await,
            ProxyType::Http => connect_http(stream, proxy, target_host, target_port).await,
        }
    })
    .await
    .map_err(|_| {
        SshError::Proxy(format!(
            "{} proxy connection timed out after {}s",
            kind,
            limit.as_secs()
        ))
    })?
}

/// 通过 SOCKS5 代理建立隧道
async fn connect_socks5(
    stream: TcpStream,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, SshError> {
    use tokio_socks::tcp::Socks5Stream;

    let target = (target_host, target_port);
    let result = match &proxy.auth {
        Some((username, password)) => {
            Socks5Stream::connect_with_password_and_socket(stream, target, username, password)
                .await
        }
        None => Socks5Stream::connect_with_socket(stream, target).await,
    };

    result.map(Socks5Stream::into_inner).map_err(|e| {
        let message = e.to_string();
        if message.contains("auth") {
            SshError::Proxy(format!("SOCKS5 proxy authentication failed: {}", e))
        } else {
            SshError::Proxy(format!("SOCKS5 proxy connection failed: {}", e))
        }
    })
}

/// 通过 SOCKS4(a) 代理建立隧道（目标为域名时自动使用 SOCKS4a）
async fn connect_socks4(
    stream: TcpStream,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, SshError> {
    use tokio_socks::tcp::Socks4Stream;

    let target = (target_host, target_port);
    let result = match &proxy.auth {
        Some((user_id, _)) => {
            Socks4Stream::connect_with_userid_and_socket(stream, target, user_id).await
        }
        None => Socks4Stream::connect_with_socket(stream, target).await,
    };

    result
        .map(Socks4Stream::into_inner)
        .map_err(|e| SshError::Proxy(format!("SOCKS4 proxy connection failed: {}", e)))
}

/// 通过 HTTP CONNECT 代理建立隧道
async fn connect_http(
    mut stream: TcpStream,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, SshError> {
    use async_http_proxy::{http_connect_tokio, http_connect_tokio_with_basic_auth};

    let result = match &proxy.auth {
        Some((username, password)) => {
            http_connect_tokio_with_basic_auth(
                &mut stream,
                target_host,
                target_port,
                username,
                password,
            )
            .await
        }
        None => http_connect_tokio(&mut stream, target_host, target_port).await,
    };

    result.map_err(|e| {
        let message = e.to_string();
        if message.contains("407") || message.contains("Proxy Authentication Required") {
            SshError::Proxy("HTTP proxy authentication failed (407)".to_string())
        } else {
            SshError::Proxy(format!("HTTP CONNECT tunnel failed: {}", e))
        }
    })?;

    Ok(stream)
}
