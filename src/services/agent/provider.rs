// Provider 适配层：把 ShellMaster3 的 ResolvedProvider 映射为 rig-core 的 DynModel<Completion>

use rig_core::completion::{Effort, GenerationOptions, OnUnsupported, Reasoning};
use rig_core::operation::Completion;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::gemini::GeminiConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::DynModel;

use crate::models::settings::ResolvedProvider;
use crate::models::{AgentReasoningEffort, ApiFormat};

/// 根据用户配置的供应商与模型，构建统一动态分发模型。
pub fn build_model(provider: &ResolvedProvider) -> Result<DynModel<Completion>, String> {
    let trimmed_key = provider.api_key.trim();
    if provider.requires_api_key && trimmed_key.is_empty() {
        return Err("API key 为空".to_string());
    }
    let effective_key = if trimmed_key.is_empty() {
        // 本地 OpenAI 兼容服务（如 Ollama / LM Studio）不校验密钥，提供占位符满足 SDK 非空要求
        "local-no-key".to_string()
    } else {
        trimmed_key.to_string()
    };
    let model = provider.model.trim();
    if model.is_empty() {
        return Err("未指定模型名称".to_string());
    }
    let base = provider.base_url.trim().trim_end_matches('/');

    let dyn_model: DynModel<Completion> = match provider.format {
        ApiFormat::OpenAiChat => {
            let mut cfg = OpenAIConfig::new(effective_key);
            if !base.is_empty() {
                cfg = cfg.with_base_url(base);
            }
            cfg.client().chat(model).into()
        }
        ApiFormat::OpenAiResponses => {
            let mut cfg = OpenAIConfig::new(effective_key);
            if !base.is_empty() {
                cfg = cfg.with_base_url(base);
            }
            cfg.client().responses(model).into()
        }
        ApiFormat::Anthropic => {
            let mut cfg = AnthropicConfig::new(effective_key);
            if !base.is_empty() {
                cfg = cfg.with_base_url(base);
            }
            cfg.client().completion(model).into()
        }
        ApiFormat::Gemini => {
            let mut cfg = GeminiConfig::new(effective_key);
            if !base.is_empty() {
                // rig-core 的 Gemini wire 会自行拼接 `/v1beta/models/{model}:...`，
                // 因此如果用户填写的 base_url 已包含 `/v1beta` 或 `/v1`，需剥离避免重复路径。
                let root = strip_gemini_version_suffix(base);
                cfg = cfg.with_base_url(root);
            }
            cfg.client().completion(model).into()
        }
    };

    Ok(dyn_model)
}

/// 将用户选择的思考深度转换为 `rig-core` 跨厂商 `GenerationOptions`。
///
/// - `Auto` 返回 `None`：完全不注入思考深度字段，遵循模型默认行为。
/// - 针对仅接受 token 预算的模型（如 Gemini 2.5、Claude 3.7），将 Low/Medium/High 自动折算为预算值。
/// - 统一开启 `OnUnsupported::Ignore`，切到不支持该参数的普通模型或中转站时自动跳过不报错。
pub fn build_generation_options(
    effort: AgentReasoningEffort,
    provider: &ResolvedProvider,
) -> Option<GenerationOptions> {
    if effort == AgentReasoningEffort::Auto {
        return None;
    }

    let model_lower = provider.model.trim().to_ascii_lowercase();
    let uses_budget = matches!(provider.format, ApiFormat::Gemini)
        && model_lower.contains("gemini-2.5")
        || matches!(provider.format, ApiFormat::Anthropic)
            && (model_lower.contains("claude-3-7") || model_lower.contains("claude-3.7"));

    let reasoning = match effort {
        AgentReasoningEffort::Auto => return None,
        AgentReasoningEffort::Off => Reasoning::Off,
        AgentReasoningEffort::Low => {
            if uses_budget {
                Reasoning::Budget { tokens: 1024 }
            } else {
                Reasoning::Effort(Effort::Low)
            }
        }
        AgentReasoningEffort::Medium => {
            if uses_budget {
                Reasoning::Budget { tokens: 8192 }
            } else {
                Reasoning::Effort(Effort::Medium)
            }
        }
        AgentReasoningEffort::High => {
            if uses_budget {
                Reasoning::Budget { tokens: 24576 }
            } else {
                Reasoning::Effort(Effort::High)
            }
        }
    };

    Some(
        GenerationOptions::default()
            .reasoning(reasoning)
            .on_unsupported(OnUnsupported::Ignore),
    )
}

fn strip_gemini_version_suffix(base: &str) -> &str {
    let trimmed = base.trim_end_matches('/');
    for suffix in ["/v1beta", "/v1alpha", "/v1"] {
        if let Some(stripped) = trimmed.strip_suffix(suffix) {
            return stripped;
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_provider(format: ApiFormat, model: &str) -> ResolvedProvider {
        ResolvedProvider {
            name: "test".to_string(),
            api_key: "sk-test".to_string(),
            base_url: String::new(),
            model: model.to_string(),
            models: vec![model.to_string()],
            format,
            verified: true,
            requires_api_key: true,
        }
    }

    #[test]
    fn local_provider_builds_without_api_key() {
        let local = ResolvedProvider {
            name: "Ollama".to_string(),
            api_key: String::new(),
            base_url: "http://localhost:11434/v1".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            models: vec!["qwen2.5-coder:7b".to_string()],
            format: ApiFormat::OpenAiChat,
            verified: true,
            requires_api_key: false,
        };
        assert!(build_model(&local).is_ok());
    }

    #[test]
    fn strip_gemini_suffix_handles_v1beta() {
        assert_eq!(
            strip_gemini_version_suffix("https://generativelanguage.googleapis.com/v1beta"),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            strip_gemini_version_suffix("https://proxy.example.com/gemini/v1beta/"),
            "https://proxy.example.com/gemini"
        );
        assert_eq!(
            strip_gemini_version_suffix("https://generativelanguage.googleapis.com"),
            "https://generativelanguage.googleapis.com"
        );
    }

    #[test]
    fn generation_options_maps_effort_and_budget_models() {
        let openai = fake_provider(ApiFormat::OpenAiResponses, "o3");
        assert!(build_generation_options(AgentReasoningEffort::Auto, &openai).is_none());

        let high = build_generation_options(AgentReasoningEffort::High, &openai).unwrap();
        assert_eq!(high.reasoning, Some(Reasoning::Effort(Effort::High)));
        assert_eq!(high.on_unsupported, Some(OnUnsupported::Ignore));

        let off = build_generation_options(AgentReasoningEffort::Off, &openai).unwrap();
        assert_eq!(off.reasoning, Some(Reasoning::Off));

        // Gemini 2.5 uses token budget; Gemini 3 uses effort levels
        let gemini25 = fake_provider(ApiFormat::Gemini, "gemini-2.5-pro");
        let med_g25 = build_generation_options(AgentReasoningEffort::Medium, &gemini25).unwrap();
        assert_eq!(med_g25.reasoning, Some(Reasoning::Budget { tokens: 8192 }));

        let gemini3 = fake_provider(ApiFormat::Gemini, "gemini-3-flash-preview");
        let low_g3 = build_generation_options(AgentReasoningEffort::Low, &gemini3).unwrap();
        assert_eq!(low_g3.reasoning, Some(Reasoning::Effort(Effort::Low)));
    }
}
