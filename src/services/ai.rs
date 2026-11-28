// AI 供应商公共消息类型、连通性探测与模型列表拉取
//
// 多轮对话与工具调用引擎位于 `crate::services::agent`（基于 rig-core + rig-agent）；
// 本模块提供共享消息角色定义、`fetch_models` 远端模型拉取以及设置页的 `test_connection` 连通性探测。

use crate::models::ApiFormat;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    #[error("Base URL 为空")]
    MissingBaseUrl,
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

/// 从 `/models` 响应体解析可用模型 ID 列表（兼容 OpenAI/Anthropic 与 Gemini 格式）
pub(crate) fn parse_models_response(format: ApiFormat, body: &str) -> Vec<String> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    match format {
        ApiFormat::Gemini => {
            if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(raw) = item.get("name").and_then(|v| v.as_str()) {
                        let id = raw.strip_prefix("models/").unwrap_or(raw).trim();
                        if !id.is_empty() && seen.insert(id.to_string()) {
                            out.push(id.to_string());
                        }
                    }
                }
            }
        }
        ApiFormat::OpenAiChat | ApiFormat::OpenAiResponses | ApiFormat::Anthropic => {
            if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        let id = id.trim();
                        if !id.is_empty() && seen.insert(id.to_string()) {
                            out.push(id.to_string());
                        }
                    }
                }
            } else if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                // 兼容部分本地服务（如 Ollama /api/tags 风格）的返回结构
                for item in arr {
                    if let Some(id) = item
                        .get("id")
                        .or_else(|| item.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        let id = id.trim();
                        if !id.is_empty() && seen.insert(id.to_string()) {
                            out.push(id.to_string());
                        }
                    }
                }
            }
        }
    }

    out.sort();
    out
}

fn build_models_request(
    client: &reqwest::Client,
    format: ApiFormat,
    api_key: &str,
    base: &str,
) -> reqwest::RequestBuilder {
    let key = api_key.trim();
    match format {
        ApiFormat::OpenAiChat | ApiFormat::OpenAiResponses => {
            let req = client.get(format!("{}/models", base));
            if key.is_empty() {
                req
            } else {
                req.bearer_auth(key)
            }
        }
        ApiFormat::Anthropic => {
            let base_v1 = normalize_anthropic_v1(base);
            let req = client
                .get(format!("{}/models", base_v1))
                .header("anthropic-version", "2023-06-01");
            if key.is_empty() {
                req
            } else {
                req.header("x-api-key", key)
            }
        }
        ApiFormat::Gemini => {
            if key.is_empty() {
                client.get(format!("{}/models", base))
            } else {
                client.get(format!("{}/models?key={}", base, key))
            }
        }
    }
}

/// 部分 Anthropic 兼容端点不带 `/v1` 后缀（如 `https://open.bigmodel.cn/api/anthropic`），
/// 补齐 `/v1` 以匹配 `/v1/models` 与 `/v1/messages`。
pub(crate) fn normalize_anthropic_v1(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{}/v1", trimmed)
    }
}

/// 从远端供应商拉取可用模型列表
pub async fn fetch_models(
    format: ApiFormat,
    api_key: &str,
    base_url: &str,
    requires_api_key: bool,
) -> Result<Vec<String>, AiError> {
    if requires_api_key && api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AiError::MissingBaseUrl);
    }
    let client = build_client(15)?;
    let resp = build_models_request(&client, format, api_key, base)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AiError::Provider {
            status: status.as_u16(),
            body,
        });
    }
    Ok(parse_models_response(format, &body))
}

/// 测试连通性：
/// 1. 优先请求 `GET /models`，2xx 时顺带返回发现的模型列表；
/// 2. 遇到 401/403 直接判定鉴权失败；
/// 3. 遇到 404/405/501（常见于 Coding Plan 订阅端点或精简中转站未实现 `/models`）且提供了模型名时，
///    自动回退为 `max_tokens=1` 的轻量对话探测。
pub async fn test_connection(
    format: ApiFormat,
    api_key: &str,
    base_url: &str,
    probe_model: &str,
    requires_api_key: bool,
) -> Result<Vec<String>, AiError> {
    if requires_api_key && api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AiError::MissingBaseUrl);
    }
    let client = build_client(20)?;

    let resp = build_models_request(&client, format, api_key, base)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        return Ok(parse_models_response(format, &body));
    }

    let code = status.as_u16();
    let should_fallback = matches!(code, 404 | 405 | 501) && !probe_model.trim().is_empty();
    if !should_fallback {
        return Err(AiError::Provider { status: code, body });
    }

    probe_minimal_completion(&client, format, api_key, base, probe_model.trim()).await?;
    Ok(Vec::new())
}

async fn probe_minimal_completion(
    client: &reqwest::Client,
    format: ApiFormat,
    api_key: &str,
    base: &str,
    model: &str,
) -> Result<(), AiError> {
    let key = api_key.trim();
    let req = match format {
        ApiFormat::OpenAiChat => {
            let mut r = client
                .post(format!("{}/chat/completions", base))
                .json(&serde_json::json!({
                    "model": model,
                    "messages": [{ "role": "user", "content": "hi" }],
                    "max_tokens": 1
                }));
            if !key.is_empty() {
                r = r.bearer_auth(key);
            }
            r
        }
        ApiFormat::OpenAiResponses => {
            let mut r = client
                .post(format!("{}/responses", base))
                .json(&serde_json::json!({
                    "model": model,
                    "input": "hi",
                    "max_output_tokens": 16
                }));
            if !key.is_empty() {
                r = r.bearer_auth(key);
            }
            r
        }
        ApiFormat::Anthropic => {
            let base_v1 = normalize_anthropic_v1(base);
            let mut r = client
                .post(format!("{}/messages", base_v1))
                .header("anthropic-version", "2023-06-01")
                .json(&serde_json::json!({
                    "model": model,
                    "max_tokens": 1,
                    "messages": [{ "role": "user", "content": "hi" }]
                }));
            if !key.is_empty() {
                r = r.header("x-api-key", key);
            }
            r
        }
        ApiFormat::Gemini => {
            let url = if key.is_empty() {
                format!("{}/models/{}:generateContent", base, model)
            } else {
                format!("{}/models/{}:generateContent?key={}", base, model, key)
            };
            client.post(url).json(&serde_json::json!({
                "contents": [{ "parts": [{ "text": "hi" }] }],
                "generationConfig": { "maxOutputTokens": 1 }
            }))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_and_gemini_model_lists() {
        let openai_body = r#"{"data":[{"id":"glm-4.7"},{"id":"glm-4.5"},{"id":"glm-4.7"}]}"#;
        assert_eq!(
            parse_models_response(ApiFormat::OpenAiChat, openai_body),
            vec!["glm-4.5".to_string(), "glm-4.7".to_string()]
        );

        let gemini_body =
            r#"{"models":[{"name":"models/gemini-2.5-pro"},{"name":"models/gemini-2.5-flash"}]}"#;
        assert_eq!(
            parse_models_response(ApiFormat::Gemini, gemini_body),
            vec![
                "gemini-2.5-flash".to_string(),
                "gemini-2.5-pro".to_string()
            ]
        );
    }

    #[test]
    fn normalizes_anthropic_v1_prefix() {
        assert_eq!(
            normalize_anthropic_v1("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(
            normalize_anthropic_v1("https://open.bigmodel.cn/api/anthropic/"),
            "https://open.bigmodel.cn/api/anthropic/v1"
        );
    }
}
