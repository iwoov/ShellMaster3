// AI 供应商接口与实现：OpenAI / Gemini / Claude / DeepSeek
//
// 提供两个能力：
// - test_connection: 通过 list-models 端点验证 api_key
// - chat_completion: 非流式发送一组消息，返回助手回复内容
//
// 所有调用都在 SshManager 的 tokio runtime 中执行；GPUI 侧通过 channel 接收结果。

use crate::models::{AiProviderConfig, AiProviderId};
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

impl ChatRole {
    fn as_openai_str(self) -> &'static str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

/// 助手回复：正文 + 可选的思考过程（仅 DeepSeek-R1 等模型返回）
#[derive(Clone, Debug, Default)]
pub struct ChatResponse {
    pub content: String,
    pub reasoning: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("API key 为空")]
    MissingKey,
    #[error("HTTP 请求失败: {0}")]
    Http(String),
    #[error("响应解析失败: {0}")]
    Parse(String),
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
    provider: AiProviderId,
    cfg: &AiProviderConfig,
) -> Result<(), AiError> {
    if cfg.api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let client = build_client(15)?;
    let base = if cfg.base_url.is_empty() {
        provider.default_base_url().to_string()
    } else {
        cfg.base_url.trim_end_matches('/').to_string()
    };

    let req = match provider {
        AiProviderId::OpenAi => client
            .get(format!("{}/models", base))
            .bearer_auth(&cfg.api_key),
        AiProviderId::DeepSeek => client
            .get(format!("{}/models", base))
            .bearer_auth(&cfg.api_key),
        AiProviderId::Claude => client
            .get(format!("{}/models", base))
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", "2023-06-01"),
        AiProviderId::Gemini => client.get(format!("{}/models?key={}", base, cfg.api_key)),
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

/// 发送一轮对话，返回助手文本（含可选 reasoning）
pub async fn chat_completion(
    provider: AiProviderId,
    cfg: &AiProviderConfig,
    messages: &[ChatMessage],
) -> Result<ChatResponse, AiError> {
    if cfg.api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let client = build_client(60)?;
    let base = if cfg.base_url.is_empty() {
        provider.default_base_url().to_string()
    } else {
        cfg.base_url.trim_end_matches('/').to_string()
    };
    let model = if cfg.model.is_empty() {
        provider.default_model().to_string()
    } else {
        cfg.model.clone()
    };

    match provider {
        AiProviderId::OpenAi | AiProviderId::DeepSeek => {
            chat_openai_compatible(&client, &base, &cfg.api_key, &model, messages).await
        }
        AiProviderId::Claude => chat_anthropic(&client, &base, &cfg.api_key, &model, messages).await,
        AiProviderId::Gemini => chat_gemini(&client, &base, &cfg.api_key, &model, messages).await,
    }
}

// ---------------------- OpenAI / DeepSeek (OpenAI 兼容) ----------------------

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    /// DeepSeek-R1 / 通义等模型返回的思考过程
    #[serde(default)]
    reasoning_content: Option<String>,
}

async fn chat_openai_compatible(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<ChatResponse, AiError> {
    let payload = OpenAiChatRequest {
        model,
        messages: messages
            .iter()
            .map(|m| OpenAiMessage {
                role: m.role.as_openai_str(),
                content: &m.content,
            })
            .collect(),
        stream: false,
    };

    let resp = client
        .post(format!("{}/chat/completions", base))
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider {
            status: status.as_u16(),
            body,
        });
    }
    let parsed: OpenAiChatResponse = resp.json().await.map_err(|e| AiError::Parse(e.to_string()))?;
    let msg = parsed.choices.into_iter().next().map(|c| c.message);
    Ok(ChatResponse {
        content: msg.as_ref().and_then(|m| m.content.clone()).unwrap_or_default(),
        reasoning: msg.and_then(|m| m.reasoning_content).filter(|s| !s.is_empty()),
    })
}

// ---------------------- Anthropic Claude ----------------------

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

async fn chat_anthropic(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<ChatResponse, AiError> {
    // 合并所有 system 消息到 top-level system 字段
    let system: Option<String> = {
        let s: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .collect();
        if s.is_empty() {
            None
        } else {
            Some(s.join("\n\n"))
        }
    };
    let msgs: Vec<AnthropicMessage> = messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| AnthropicMessage {
            role: match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "user",
            },
            content: &m.content,
        })
        .collect();

    let payload = AnthropicRequest {
        model,
        max_tokens: 1024,
        messages: msgs,
        system,
    };

    let resp = client
        .post(format!("{}/messages", base))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider {
            status: status.as_u16(),
            body,
        });
    }
    let parsed: AnthropicResponse = resp.json().await.map_err(|e| AiError::Parse(e.to_string()))?;
    let text = parsed
        .content
        .into_iter()
        .filter(|b| b.block_type == "text")
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");
    Ok(ChatResponse { content: text, reasoning: None })
}

// ---------------------- Google Gemini ----------------------

#[derive(Serialize)]
struct GeminiRequest<'a> {
    contents: Vec<GeminiContent<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction<'a>>,
}

#[derive(Serialize)]
struct GeminiContent<'a> {
    role: &'a str,
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiSystemInstruction<'a> {
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContentOut>,
}

#[derive(Deserialize)]
struct GeminiContentOut {
    parts: Option<Vec<GeminiPartOut>>,
}

#[derive(Deserialize)]
struct GeminiPartOut {
    text: Option<String>,
}

async fn chat_gemini(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<ChatResponse, AiError> {
    let system_text: Option<String> = {
        let s: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .collect();
        if s.is_empty() {
            None
        } else {
            Some(s.join("\n\n"))
        }
    };

    let contents: Vec<GeminiContent> = messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| GeminiContent {
            role: match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "model",
                ChatRole::System => "user",
            },
            parts: vec![GeminiPart { text: &m.content }],
        })
        .collect();

    let payload = GeminiRequest {
        contents,
        system_instruction: system_text.as_deref().map(|t| GeminiSystemInstruction {
            parts: vec![GeminiPart { text: t }],
        }),
    };

    let url = format!("{}/models/{}:generateContent?key={}", base, model, api_key);
    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider {
            status: status.as_u16(),
            body,
        });
    }
    let parsed: GeminiResponse = resp.json().await.map_err(|e| AiError::Parse(e.to_string()))?;
    let text = parsed
        .candidates
        .unwrap_or_default()
        .into_iter()
        .next()
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .map(|parts| {
            parts
                .into_iter()
                .filter_map(|p| p.text)
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    Ok(ChatResponse { content: text, reasoning: None })
}

// ============================================================================
//                              流式调用 (SSE)
// ============================================================================

/// 流式事件
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// 思考过程增量（DeepSeek-R1 等）
    ReasoningDelta(String),
    /// 正文增量
    ContentDelta(String),
    /// 完成
    Done,
}

/// 流式发送一轮对话，每次增量通过 tx 发出。最终 Ok(()) 或 Err(AiError)。
pub async fn chat_completion_stream(
    provider: AiProviderId,
    cfg: &AiProviderConfig,
    messages: &[ChatMessage],
    tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), AiError> {
    if cfg.api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AiError::Http(e.to_string()))?;
    let base = if cfg.base_url.is_empty() {
        provider.default_base_url().to_string()
    } else {
        cfg.base_url.trim_end_matches('/').to_string()
    };
    let model = if cfg.model.is_empty() {
        provider.default_model().to_string()
    } else {
        cfg.model.clone()
    };

    let result = match provider {
        AiProviderId::OpenAi | AiProviderId::DeepSeek => {
            stream_openai_compatible(&client, &base, &cfg.api_key, &model, messages, &tx).await
        }
        AiProviderId::Claude => {
            stream_anthropic(&client, &base, &cfg.api_key, &model, messages, &tx).await
        }
        AiProviderId::Gemini => {
            stream_gemini(&client, &base, &cfg.api_key, &model, messages, &tx).await
        }
    };
    let _ = tx.send(StreamEvent::Done);
    result
}

/// SSE 按行解析器
async fn for_each_sse_event<F>(mut response: reqwest::Response, mut handle: F) -> Result<(), AiError>
where
    F: FnMut(&str, &str) -> bool,
{
    let mut buf = String::new();
    let mut current_event = String::new();
    let mut current_data = String::new();

    while let Some(chunk) = response.chunk().await.map_err(|e| AiError::Http(e.to_string()))? {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        loop {
            let Some(nl) = buf.find('\n') else { break };
            let line = buf[..nl].trim_end_matches('\r').to_string();
            buf.drain(..=nl);

            if line.is_empty() {
                if !current_data.is_empty() || !current_event.is_empty() {
                    if !handle(&current_event, &current_data) {
                        return Ok(());
                    }
                }
                current_event.clear();
                current_data.clear();
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                if !current_data.is_empty() {
                    current_data.push('\n');
                }
                current_data.push_str(rest);
            } else if let Some(rest) = line.strip_prefix("event:") {
                current_event = rest.trim().to_string();
            }
        }
    }
    if !current_data.is_empty() {
        let _ = handle(&current_event, &current_data);
    }
    Ok(())
}

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
}

#[derive(Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

async fn stream_openai_compatible(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
    tx: &tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), AiError> {
    let payload = OpenAiChatRequest {
        model,
        messages: messages
            .iter()
            .map(|m| OpenAiMessage {
                role: m.role.as_openai_str(),
                content: &m.content,
            })
            .collect(),
        stream: true,
    };
    let resp = client
        .post(format!("{}/chat/completions", base))
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider { status, body });
    }
    for_each_sse_event(resp, |_event, data| {
        if data == "[DONE]" {
            return false;
        }
        if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(data) {
            for ch in chunk.choices {
                if let Some(r) = ch.delta.reasoning_content {
                    if !r.is_empty() {
                        let _ = tx.send(StreamEvent::ReasoningDelta(r));
                    }
                }
                if let Some(c) = ch.delta.content {
                    if !c.is_empty() {
                        let _ = tx.send(StreamEvent::ContentDelta(c));
                    }
                }
            }
        }
        true
    })
    .await
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(default)]
    delta: Option<AnthropicTextDelta>,
}

#[derive(Deserialize)]
struct AnthropicTextDelta {
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    text: String,
}

async fn stream_anthropic(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
    tx: &tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), AiError> {
    let system: Option<String> = {
        let s: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .collect();
        if s.is_empty() { None } else { Some(s.join("\n\n")) }
    };
    let msgs: Vec<AnthropicMessage> = messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| AnthropicMessage {
            role: match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "user",
            },
            content: &m.content,
        })
        .collect();

    #[derive(Serialize)]
    struct StreamReq<'a> {
        model: &'a str,
        max_tokens: u32,
        messages: Vec<AnthropicMessage<'a>>,
        stream: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        system: Option<String>,
    }
    let payload = StreamReq {
        model,
        max_tokens: 1024,
        messages: msgs,
        stream: true,
        system,
    };
    let resp = client
        .post(format!("{}/messages", base))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider { status, body });
    }
    for_each_sse_event(resp, |event, data| {
        if event != "content_block_delta" {
            return true;
        }
        if let Ok(parsed) = serde_json::from_str::<AnthropicStreamDelta>(data) {
            if let Some(d) = parsed.delta {
                if d.type_ == "text_delta" && !d.text.is_empty() {
                    let _ = tx.send(StreamEvent::ContentDelta(d.text));
                }
            }
        }
        true
    })
    .await
}

async fn stream_gemini(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
    tx: &tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), AiError> {
    let system_text: Option<String> = {
        let s: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .collect();
        if s.is_empty() { None } else { Some(s.join("\n\n")) }
    };
    let contents: Vec<GeminiContent> = messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| GeminiContent {
            role: match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "model",
                ChatRole::System => "user",
            },
            parts: vec![GeminiPart { text: &m.content }],
        })
        .collect();
    let payload = GeminiRequest {
        contents,
        system_instruction: system_text.as_deref().map(|t| GeminiSystemInstruction {
            parts: vec![GeminiPart { text: t }],
        }),
    };
    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={}",
        base, model, api_key
    );
    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::Provider { status, body });
    }
    for_each_sse_event(resp, |_event, data| {
        if data.is_empty() {
            return true;
        }
        if let Ok(parsed) = serde_json::from_str::<GeminiResponse>(data) {
            if let Some(cands) = parsed.candidates {
                for cand in cands {
                    if let Some(content) = cand.content {
                        if let Some(parts) = content.parts {
                            for p in parts {
                                if let Some(t) = p.text {
                                    if !t.is_empty() {
                                        let _ = tx.send(StreamEvent::ContentDelta(t));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        true
    })
    .await
}
