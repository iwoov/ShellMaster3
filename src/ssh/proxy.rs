// 代理连接模块
// 支持 HTTP CONNECT 和 SOCKS5 代理

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;

use super::config::{ProxyConfig, ProxyType};
use super::error::SshError;

/// 通过代理连接到目标主机
///
/// 根据代理类型（HTTP/SOCKS5）建立隧道连接
pub async fn connect_via_proxy(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
    connect_timeout: Duration,
) -> Result<TcpStream, SshError> {
    // 先连接到代理服务器
    let proxy_addr = format!("{}:{}", proxy.host, proxy.port);
    let proxy_socket: SocketAddr = proxy_addr
        .parse()
        .or_else(|_| {
            // 如果不是直接的 IP:PORT，尝试 DNS 解析
            use std::net::ToSocketAddrs;
            proxy_addr
                .to_socket_addrs()
                .map_err(|e| SshError::Proxy(format!("Failed to resolve proxy address: {}", e)))?
                .next()
                .ok_or_else(|| SshError::Proxy("No valid proxy address found".to_string()))
        })?;

    match proxy.proxy_type {
        ProxyType::Socks5 => {
            connect_socks5(proxy_socket, proxy, target_host, target_port, connect_timeout).await
        }
        ProxyType::Http => {
            connect_http(proxy_socket, proxy, target_host, target_port, connect_timeout).await
        }
    }
}

/// 通过 SOCKS5 代理连接
async fn connect_socks5(
    proxy_addr: SocketAddr,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
    connect_timeout: Duration,
) -> Result<TcpStream, SshError> {
    use tokio_socks::tcp::Socks5Stream;

    let target = (target_host, target_port);

    let stream = if let Some((username, password)) = &proxy.auth {
        // 带认证的 SOCKS5 连接
        timeout(
            connect_timeout,
            Socks5Stream::connect_with_password(proxy_addr, target, username, password),
        )
        .await
        .map_err(|_| SshError::Proxy(format!("SOCKS5 proxy connection timeout")))?
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("authentication") || err_str.contains("auth") {
                SshError::Proxy(format!("SOCKS5 proxy authentication failed: {}", e))
            } else {
                SshError::Proxy(format!("SOCKS5 proxy connection failed: {}", e))
            }
        })?
    } else {
        // 无认证的 SOCKS5 连接
        timeout(
            connect_timeout,
            Socks5Stream::connect(proxy_addr, target),
        )
        .await
        .map_err(|_| SshError::Proxy(format!("SOCKS5 proxy connection timeout")))?
        .map_err(|e| SshError::Proxy(format!("SOCKS5 proxy connection failed: {}", e)))?
    };

    Ok(stream.into_inner())
}

/// 通过 HTTP CONNECT 代理连接
async fn connect_http(
    proxy_addr: SocketAddr,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
    connect_timeout: Duration,
) -> Result<TcpStream, SshError> {
    use async_http_proxy::{http_connect_tokio, http_connect_tokio_with_basic_auth};

    // 先建立到代理的 TCP 连接
    let mut stream = timeout(connect_timeout, TcpStream::connect(proxy_addr))
        .await
        .map_err(|_| SshError::Proxy("HTTP proxy connection timeout".to_string()))?
        .map_err(|e| SshError::Proxy(format!("Failed to connect to HTTP proxy: {}", e)))?;

    // 发送 HTTP CONNECT 请求建立隧道
    if let Some((username, password)) = &proxy.auth {
        // 带认证
        timeout(
            connect_timeout,
            http_connect_tokio_with_basic_auth(&mut stream, target_host, target_port, username, password),
        )
        .await
        .map_err(|_| SshError::Proxy("HTTP CONNECT tunnel timeout".to_string()))?
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("407") || err_str.contains("Proxy Authentication Required") {
                SshError::Proxy("HTTP proxy authentication failed (407)".to_string())
            } else {
                SshError::Proxy(format!("HTTP CONNECT tunnel failed: {}", e))
            }
        })?;
    } else {
        // 无认证
        timeout(
            connect_timeout,
            http_connect_tokio(&mut stream, target_host, target_port),
        )
        .await
        .map_err(|_| SshError::Proxy("HTTP CONNECT tunnel timeout".to_string()))?
        .map_err(|e| SshError::Proxy(format!("HTTP CONNECT tunnel failed: {}", e)))?;
    }

    Ok(stream)
}
