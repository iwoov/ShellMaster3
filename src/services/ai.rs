// AI 供应商公共消息类型与连通性测试接口：OpenAI / Gemini / Claude / DeepSeek
//
// 多轮对话与工具调用引擎位于 `crate::services::agent`（基于 rig-core + rig-agent）；
// 本模块保留共享消息角色定义以及设置页的 `test_connection` 连通性探测。

use crate::models::ApiFormat;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 一条对话消息（用户/助手/系统）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("API key 为空")]
    MissingKey,
    #[error("HTTP 请求失败: {0}")]
    Http(String),
    #[error("供应商返回错误 ({status}): {body}")]
    Provider { status: u16, body: String },
}

fn build_client(timeout_secs: u64) -> Result<reqwest::Client, AiError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AiError::Http(e.to_string()))
}

/// 测试连通性（list-models 端点，2xx 即通过）
pub async fn test_connection(
    format: ApiFormat,
    api_key: &str,
    base_url: &str,
) -> Result<(), AiError> {
    if api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let client = build_client(15)?;
    let base = base_url.trim_end_matches('/').to_string();

    let req = match format {
        ApiFormat::OpenAiChat | ApiFormat::OpenAiResponses => {
            client.get(format!("{}/models", base)).bearer_auth(api_key)
        }
        ApiFormat::Anthropic => client
            .get(format!("{}/models", base))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
        ApiFormat::Gemini => client.get(format!("{}/models?key={}", base, api_key)),
    };

    let resp = req.send().await.map_err(|e| AiError::Http(e.to_string()))?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(AiError::Provider {
            status: status.as_u16(),
            body,
        })
    }
}
