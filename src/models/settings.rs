// Settings 配置数据结构

use serde::{Deserialize, Serialize};

// ======================== 主配置结构 ========================

/// 当前设置文件格式版本（用于一次性迁移旧配置）
pub const CURRENT_SETTINGS_VERSION: u32 = 1;

/// 旧版本的默认心跳间隔。旧版本还设置了 30 秒空闲断开，实际断线检测靠它；
/// 去掉空闲断开后改由心跳检测，60 秒的间隔太慢。
const LEGACY_DEFAULT_KEEPALIVE_SECS: u32 = 60;

/// 应用设置（持久化用）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    /// 设置文件格式版本；旧文件没有该字段，按 0 处理
    #[serde(default)]
    pub settings_version: u32,
    pub theme: ThemeSettings,
    pub terminal: TerminalSettings,
    pub sftp: SftpSettings,
    pub monitor: MonitorSettings,
    pub connection: ConnectionSettings,
    pub sync: SyncSettings,
    pub system: SystemSettings,
    #[serde(default)]
    pub ai: AiSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            settings_version: CURRENT_SETTINGS_VERSION,
            theme: ThemeSettings::default(),
            terminal: TerminalSettings::default(),
            sftp: SftpSettings::default(),
            monitor: MonitorSettings::default(),
            connection: ConnectionSettings::default(),
            sync: SyncSettings::default(),
            system: SystemSettings::default(),
            ai: AiSettings::default(),
        }
    }
}

impl AppSettings {
    /// 把旧版本的设置升级到当前格式，返回是否有改动（需要写回文件）
    pub fn migrate(&mut self) -> bool {
        if self.settings_version >= CURRENT_SETTINGS_VERSION {
            return false;
        }
        if self.settings_version < 1 {
            // v1：心跳成为唯一的断线检测手段，仍是旧默认值 60 秒的改为 15 秒
            // （连续 4 次无响应约 1 分钟判定断开）；用户自定义过的值保持不变。
            if self.connection.keepalive_interval_secs == LEGACY_DEFAULT_KEEPALIVE_SECS {
                self.connection.keepalive_interval_secs =
                    ConnectionSettings::default().keepalive_interval_secs;
            }
        }
        self.settings_version = CURRENT_SETTINGS_VERSION;
        true
    }
}

// ======================== AI 设置 ========================

/// 供应商端点预设（用于在标准按量 API、国际站、Coding Plan 订阅端点之间一键切换）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointPreset {
    pub label_zh: &'static str,
    pub label_en: &'static str,
    pub base_url: &'static str,
    pub format: ApiFormat,
}

impl EndpointPreset {
    pub fn label(&self, lang: &Language) -> &'static str {
        match lang {
            Language::Chinese => self.label_zh,
            Language::English => self.label_en,
        }
    }
}

/// 判断 Base URL 是否指向本机地址（本地模型服务如 Ollama / LM Studio 无需 API Key）
pub fn is_local_base_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("https://localhost")
        || lower.starts_with("http://127.0.0.1")
        || lower.starts_with("https://127.0.0.1")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("https://[::1]")
        || lower.starts_with("http://0.0.0.0")
}

/// 内置 AI 供应商分组：区分「Coding 订阅」「云端 API」「本地模型」
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiProviderGroup {
    Subscription,
    Api,
    Local,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AiProviderId {
    // 订阅（Coding Plan）
    ZhipuCoding,
    QwenCoding,
    KimiCoding,
    MiniMaxCoding,
    VolcengineCoding,
    // 云端 API（国际 / 国内 / 聚合）
    OpenAi,
    Claude,
    Gemini,
    DeepSeek,
    Zhipu,
    Qwen,
    Kimi,
    MiniMax,
    Volcengine,
    SiliconFlow,
    OpenRouter,
    Xai,
    Groq,
    // 本地模型
    Ollama,
    LmStudio,
}

impl AiProviderId {
    pub const SUBSCRIPTIONS: [AiProviderId; 5] = [
        AiProviderId::ZhipuCoding,
        AiProviderId::QwenCoding,
        AiProviderId::KimiCoding,
        AiProviderId::MiniMaxCoding,
        AiProviderId::VolcengineCoding,
    ];

    pub const APIS: [AiProviderId; 13] = [
        AiProviderId::OpenAi,
        AiProviderId::Claude,
        AiProviderId::Gemini,
        AiProviderId::DeepSeek,
        AiProviderId::Zhipu,
        AiProviderId::Qwen,
        AiProviderId::Kimi,
        AiProviderId::MiniMax,
        AiProviderId::Volcengine,
        AiProviderId::SiliconFlow,
        AiProviderId::OpenRouter,
        AiProviderId::Xai,
        AiProviderId::Groq,
    ];

    pub const LOCALS: [AiProviderId; 2] = [AiProviderId::Ollama, AiProviderId::LmStudio];

    pub const ALL: [AiProviderId; 20] = [
        AiProviderId::ZhipuCoding,
        AiProviderId::QwenCoding,
        AiProviderId::KimiCoding,
        AiProviderId::MiniMaxCoding,
        AiProviderId::VolcengineCoding,
        AiProviderId::OpenAi,
        AiProviderId::Claude,
        AiProviderId::Gemini,
        AiProviderId::DeepSeek,
        AiProviderId::Zhipu,
        AiProviderId::Qwen,
        AiProviderId::Kimi,
        AiProviderId::MiniMax,
        AiProviderId::Volcengine,
        AiProviderId::SiliconFlow,
        AiProviderId::OpenRouter,
        AiProviderId::Xai,
        AiProviderId::Groq,
        AiProviderId::Ollama,
        AiProviderId::LmStudio,
    ];

    pub fn group(&self) -> AiProviderGroup {
        match self {
            AiProviderId::ZhipuCoding
            | AiProviderId::QwenCoding
            | AiProviderId::KimiCoding
            | AiProviderId::MiniMaxCoding
            | AiProviderId::VolcengineCoding => AiProviderGroup::Subscription,
            AiProviderId::Ollama | AiProviderId::LmStudio => AiProviderGroup::Local,
            _ => AiProviderGroup::Api,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AiProviderId::ZhipuCoding => "智谱 Coding Plan",
            AiProviderId::QwenCoding => "百炼 Coding Plan",
            AiProviderId::KimiCoding => "Kimi Coding",
            AiProviderId::MiniMaxCoding => "MiniMax Coding",
            AiProviderId::VolcengineCoding => "方舟 Coding Plan",
            AiProviderId::OpenAi => "OpenAI",
            AiProviderId::Claude => "Claude",
            AiProviderId::Gemini => "Gemini",
            AiProviderId::DeepSeek => "DeepSeek",
            AiProviderId::Zhipu => "智谱 GLM",
            AiProviderId::Qwen => "阿里云百炼 / Qwen",
            AiProviderId::Kimi => "Kimi (Moonshot)",
            AiProviderId::MiniMax => "MiniMax",
            AiProviderId::Volcengine => "火山方舟 / 豆包",
            AiProviderId::SiliconFlow => "硅基流动",
            AiProviderId::OpenRouter => "OpenRouter",
            AiProviderId::Xai => "xAI (Grok)",
            AiProviderId::Groq => "Groq",
            AiProviderId::Ollama => "Ollama",
            AiProviderId::LmStudio => "LM Studio",
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            AiProviderId::ZhipuCoding => "zhipu_coding",
            AiProviderId::QwenCoding => "qwen_coding",
            AiProviderId::KimiCoding => "kimi_coding",
            AiProviderId::MiniMaxCoding => "minimax_coding",
            AiProviderId::VolcengineCoding => "volcengine_coding",
            AiProviderId::OpenAi => "openai",
            AiProviderId::Claude => "claude",
            AiProviderId::Gemini => "gemini",
            AiProviderId::DeepSeek => "deepseek",
            AiProviderId::Zhipu => "zhipu",
            AiProviderId::Qwen => "qwen",
            AiProviderId::Kimi => "kimi",
            AiProviderId::MiniMax => "minimax",
            AiProviderId::Volcengine => "volcengine",
            AiProviderId::SiliconFlow => "siliconflow",
            AiProviderId::OpenRouter => "openrouter",
            AiProviderId::Xai => "xai",
            AiProviderId::Groq => "groq",
            AiProviderId::Ollama => "ollama",
            AiProviderId::LmStudio => "lmstudio",
        }
    }

    pub fn default_base_url(&self) -> &'static str {
        match self {
            AiProviderId::ZhipuCoding => "https://open.bigmodel.cn/api/coding/paas/v4",
            AiProviderId::QwenCoding => "https://coding.dashscope.aliyuncs.com/v1",
            AiProviderId::KimiCoding => "https://api.kimi.com/coding/v1",
            AiProviderId::MiniMaxCoding => "https://api.minimaxi.com/v1",
            AiProviderId::VolcengineCoding => "https://ark.cn-beijing.volces.com/api/coding/v3",
            AiProviderId::OpenAi => "https://api.openai.com/v1",
            AiProviderId::Claude => "https://api.anthropic.com/v1",
            AiProviderId::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            AiProviderId::DeepSeek => "https://api.deepseek.com/v1",
            AiProviderId::Zhipu => "https://open.bigmodel.cn/api/paas/v4",
            AiProviderId::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            AiProviderId::Kimi => "https://api.moonshot.cn/v1",
            AiProviderId::MiniMax => "https://api.minimaxi.com/v1",
            AiProviderId::Volcengine => "https://ark.cn-beijing.volces.com/api/v3",
            AiProviderId::SiliconFlow => "https://api.siliconflow.cn/v1",
            AiProviderId::OpenRouter => "https://openrouter.ai/api/v1",
            AiProviderId::Xai => "https://api.x.ai/v1",
            AiProviderId::Groq => "https://api.groq.com/openai/v1",
            AiProviderId::Ollama => "http://localhost:11434/v1",
            AiProviderId::LmStudio => "http://localhost:1234/v1",
        }
    }

    pub fn default_model(&self) -> &'static str {
        self.default_models()[0]
    }

    /// 各供应商预置的常见模型列表（首项作为默认模型）
    pub fn default_models(&self) -> &'static [&'static str] {
        match self {
            AiProviderId::ZhipuCoding => &["glm-4.7", "glm-4.5", "glm-4.5-air"],
            AiProviderId::QwenCoding => &["qwen3-coder-plus", "qwen3-max", "qwen-plus"],
            AiProviderId::KimiCoding => &["kimi-for-coding", "kimi-k2.5", "kimi-k2-0905-preview"],
            AiProviderId::MiniMaxCoding => {
                &["MiniMax-M2.5", "MiniMax-M2.1", "MiniMax-M2.5-highspeed"]
            }
            AiProviderId::VolcengineCoding => {
                &["doubao-seed-code", "deepseek-v3-250324", "doubao-seed-1-6-250615"]
            }
            AiProviderId::OpenAi => &["gpt-4.1-mini", "gpt-4.1", "gpt-5-mini", "o4-mini"],
            AiProviderId::Claude => &[
                "claude-sonnet-4-5",
                "claude-haiku-4-5",
                "claude-opus-4-1",
            ],
            AiProviderId::Gemini => &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash"],
            AiProviderId::DeepSeek => &["deepseek-chat", "deepseek-reasoner"],
            AiProviderId::Zhipu => &["glm-4.5-air", "glm-4.7", "glm-4.5"],
            AiProviderId::Qwen => &["qwen3-coder-plus", "qwen-plus", "qwen-max", "qwen-turbo"],
            AiProviderId::Kimi => &["kimi-k2-0905-preview", "kimi-k2.5", "moonshot-v1-auto"],
            AiProviderId::MiniMax => &["MiniMax-M2.5", "MiniMax-M2.1", "MiniMax-M2.5-highspeed"],
            AiProviderId::Volcengine => &[
                "doubao-seed-1-6-250615",
                "doubao-1-5-pro-32k-250115",
                "deepseek-v3-250324",
            ],
            AiProviderId::SiliconFlow => &[
                "deepseek-ai/DeepSeek-V3",
                "deepseek-ai/DeepSeek-R1",
                "Qwen/Qwen3-Coder-480B-A35B-Instruct",
            ],
            AiProviderId::OpenRouter => &[
                "anthropic/claude-sonnet-4.5",
                "openai/gpt-4.1-mini",
                "google/gemini-2.5-flash",
                "deepseek/deepseek-chat-v3-0324",
            ],
            AiProviderId::Xai => &["grok-3-mini", "grok-3", "grok-code-fast-1"],
            AiProviderId::Groq => &[
                "llama-3.3-70b-versatile",
                "qwen/qwen3-32b",
                "openai/gpt-oss-120b",
            ],
            AiProviderId::Ollama => &["qwen2.5-coder:7b", "qwen3:8b", "llama3.1:8b"],
            AiProviderId::LmStudio => &["local-model"],
        }
    }

    /// 官方品牌 SVG 图标路径
    pub fn icon_path(&self) -> &'static str {
        use crate::constants::icons;
        match self {
            AiProviderId::OpenAi => icons::AI_OPENAI,
            AiProviderId::Gemini => icons::AI_GEMINI,
            AiProviderId::Claude => icons::AI_CLAUDE,
            AiProviderId::DeepSeek => icons::AI_DEEPSEEK,
            AiProviderId::Zhipu | AiProviderId::ZhipuCoding => icons::AI_ZHIPU,
            AiProviderId::Qwen | AiProviderId::QwenCoding => icons::AI_QWEN,
            AiProviderId::Kimi | AiProviderId::KimiCoding => icons::AI_KIMI,
            AiProviderId::MiniMax | AiProviderId::MiniMaxCoding => icons::AI_MINIMAX,
            AiProviderId::Volcengine | AiProviderId::VolcengineCoding => icons::AI_VOLCENGINE,
            AiProviderId::SiliconFlow => icons::AI_SILICONFLOW,
            AiProviderId::OpenRouter => icons::AI_OPENROUTER,
            AiProviderId::Xai => icons::AI_XAI,
            AiProviderId::Groq => icons::AI_GROQ,
            AiProviderId::Ollama => icons::AI_OLLAMA,
            AiProviderId::LmStudio => icons::AI_LMSTUDIO,
        }
    }

    /// 是否为单色矢量图标（深浅主题自动跟随前景色着色，避免深色模式变黑看不清）
    pub fn is_monochrome_icon(&self) -> bool {
        matches!(
            self,
            AiProviderId::OpenAi
                | AiProviderId::OpenRouter
                | AiProviderId::Xai
                | AiProviderId::Ollama
        )
    }

    /// 是否必须提供 API Key（本地模型如 Ollama / LM Studio 允许留空）
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, AiProviderId::Ollama | AiProviderId::LmStudio)
    }

    /// 内置供应商默认的 API 协议格式
    pub fn api_format(&self) -> ApiFormat {
        match self {
            AiProviderId::Claude => ApiFormat::Anthropic,
            AiProviderId::Gemini => ApiFormat::Gemini,
            _ => ApiFormat::OpenAiChat,
        }
    }

    /// 快捷端点预设（协议切换或国内外站切换）
    pub fn endpoint_presets(&self) -> &'static [EndpointPreset] {
        match self {
            AiProviderId::ZhipuCoding => &[
                EndpointPreset {
                    label_zh: "OpenAI 协议",
                    label_en: "OpenAI Format",
                    base_url: "https://open.bigmodel.cn/api/coding/paas/v4",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "Anthropic 协议",
                    label_en: "Anthropic Format",
                    base_url: "https://open.bigmodel.cn/api/anthropic",
                    format: ApiFormat::Anthropic,
                },
            ],
            AiProviderId::QwenCoding => &[
                EndpointPreset {
                    label_zh: "OpenAI 协议",
                    label_en: "OpenAI Format",
                    base_url: "https://coding.dashscope.aliyuncs.com/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "Anthropic 协议",
                    label_en: "Anthropic Format",
                    base_url: "https://coding.dashscope.aliyuncs.com/apps/anthropic",
                    format: ApiFormat::Anthropic,
                },
            ],
            AiProviderId::KimiCoding => &[
                EndpointPreset {
                    label_zh: "OpenAI 协议",
                    label_en: "OpenAI Format",
                    base_url: "https://api.kimi.com/coding/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "Anthropic 协议",
                    label_en: "Anthropic Format",
                    base_url: "https://api.kimi.com/coding",
                    format: ApiFormat::Anthropic,
                },
            ],
            AiProviderId::MiniMaxCoding => &[
                EndpointPreset {
                    label_zh: "国内 (OpenAI)",
                    label_en: "China (OpenAI)",
                    base_url: "https://api.minimaxi.com/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "国内 (Anthropic)",
                    label_en: "China (Anthropic)",
                    base_url: "https://api.minimaxi.com/anthropic",
                    format: ApiFormat::Anthropic,
                },
                EndpointPreset {
                    label_zh: "国际站 (OpenAI)",
                    label_en: "Global (OpenAI)",
                    base_url: "https://api.minimax.io/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "国际站 (Anthropic)",
                    label_en: "Global (Anthropic)",
                    base_url: "https://api.minimax.io/anthropic",
                    format: ApiFormat::Anthropic,
                },
            ],
            AiProviderId::VolcengineCoding => &[
                EndpointPreset {
                    label_zh: "OpenAI 协议",
                    label_en: "OpenAI Format",
                    base_url: "https://ark.cn-beijing.volces.com/api/coding/v3",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "Anthropic 协议",
                    label_en: "Anthropic Format",
                    base_url: "https://ark.cn-beijing.volces.com/api/coding",
                    format: ApiFormat::Anthropic,
                },
            ],
            AiProviderId::OpenAi => &[
                EndpointPreset {
                    label_zh: "Chat API",
                    label_en: "Chat API",
                    base_url: "https://api.openai.com/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "Responses API",
                    label_en: "Responses API",
                    base_url: "https://api.openai.com/v1",
                    format: ApiFormat::OpenAiResponses,
                },
            ],
            AiProviderId::Qwen => &[
                EndpointPreset {
                    label_zh: "国内站",
                    label_en: "China",
                    base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "国际站",
                    label_en: "International",
                    base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
                    format: ApiFormat::OpenAiChat,
                },
            ],
            AiProviderId::Kimi => &[
                EndpointPreset {
                    label_zh: "国内站",
                    label_en: "China",
                    base_url: "https://api.moonshot.cn/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "国际站",
                    label_en: "International",
                    base_url: "https://api.moonshot.ai/v1",
                    format: ApiFormat::OpenAiChat,
                },
            ],
            AiProviderId::MiniMax => &[
                EndpointPreset {
                    label_zh: "国内站 (OpenAI)",
                    label_en: "China (OpenAI)",
                    base_url: "https://api.minimaxi.com/v1",
                    format: ApiFormat::OpenAiChat,
                },
                EndpointPreset {
                    label_zh: "国内站 (Anthropic)",
                    label_en: "China (Anthropic)",
                    base_url: "https://api.minimaxi.com/anthropic",
                    format: ApiFormat::Anthropic,
                },
                EndpointPreset {
                    label_zh: "国际站 (OpenAI)",
                    label_en: "Global (OpenAI)",
                    base_url: "https://api.minimax.io/v1",
                    format: ApiFormat::OpenAiChat,
                },
            ],
            _ => &[],
        }
    }

    /// 该内置供应商是否提供多种协议格式切换（如同时支持 OpenAI 与 Anthropic 协议）
    pub fn supports_format_override(&self) -> bool {
        let presets = self.endpoint_presets();
        let default_fmt = self.api_format();
        presets.iter().any(|p| p.format != default_fmt)
    }
}

/// API 协议格式（决定请求/响应/流式如何编解码）
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApiFormat {
    /// OpenAI Chat Completions（/chat/completions）
    OpenAiChat,
    /// OpenAI Responses（/responses）
    OpenAiResponses,
    /// Google Gemini（/models/{model}:generateContent）
    Gemini,
    /// Anthropic Messages（/messages）
    Anthropic,
}

impl Default for ApiFormat {
    fn default() -> Self {
        ApiFormat::OpenAiChat
    }
}

impl ApiFormat {
    pub const ALL: [ApiFormat; 4] = [
        ApiFormat::OpenAiChat,
        ApiFormat::OpenAiResponses,
        ApiFormat::Gemini,
        ApiFormat::Anthropic,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ApiFormat::OpenAiChat => "OpenAI Chat Completions",
            ApiFormat::OpenAiResponses => "OpenAI Responses",
            ApiFormat::Gemini => "Gemini",
            ApiFormat::Anthropic => "Anthropic",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiProviderConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    /// 可选协议格式覆盖（如 Coding Plan 切换为 Anthropic / OpenAI Responses 协议）
    #[serde(default)]
    pub format: Option<ApiFormat>,
    /// 当前/默认使用的模型（= models 列表第一项）
    #[serde(default)]
    pub model: String,
    /// 该供应商可选的模型列表（侧边栏下拉用）
    #[serde(default)]
    pub models: Vec<String>,
    /// 是否已通过连通性测试，仅通过后才允许保存
    #[serde(default)]
    pub verified: bool,
}

impl AiProviderConfig {
    pub fn for_provider(id: AiProviderId) -> Self {
        let models: Vec<String> = id.default_models().iter().map(|s| s.to_string()).collect();
        Self {
            api_key: String::new(),
            base_url: id.default_base_url().to_string(),
            format: None,
            model: id.default_model().to_string(),
            models,
            verified: false,
        }
    }

    /// 实际生效的协议格式（未覆盖时使用供应商默认值）
    pub fn effective_format(&self, id: AiProviderId) -> ApiFormat {
        self.format.unwrap_or_else(|| id.api_format())
    }

    /// 返回可选模型列表：models 非空则用之，否则回退到单个 model
    pub fn model_list(&self) -> Vec<String> {
        if !self.models.is_empty() {
            self.models.clone()
        } else if !self.model.is_empty() {
            vec![self.model.clone()]
        } else {
            Vec::new()
        }
    }
}

/// 终端上下文携带设置：对话时是否把当前终端环境信息发给模型
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiContextSettings {
    /// 总开关：是否携带终端上下文
    pub enabled: bool,
    /// 携带连接信息（标签/主机/端口/用户名）
    pub server_info: bool,
    /// 携带操作系统信息（复用 Monitor 采集的 os/内核/主机名/架构）
    pub os_info: bool,
    /// 携带终端最近输出
    pub terminal_output: bool,
    /// 终端输出携带的行数
    pub output_lines: u32,
}

impl Default for AiContextSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            server_info: true,
            os_info: true,
            terminal_output: true,
            output_lines: 40,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSettings {
    /// 默认对话使用的供应商
    pub default_provider: AiProviderId,
    /// 各供应商配置（按 provider key 存储）
    pub providers: std::collections::HashMap<String, AiProviderConfig>,
    /// 全局系统提示词（每次对话注入到 history 首部）
    #[serde(default = "AiSettings::default_system_prompt")]
    pub system_prompt: String,
    /// 用户自定义供应商
    #[serde(default)]
    pub custom_providers: Vec<CustomProvider>,
    /// 终端上下文携带设置
    #[serde(default)]
    pub context: AiContextSettings,
    /// Agent 运行参数与终端操作安全策略
    #[serde(default)]
    pub agent: crate::models::AgentSettings,
}

impl Default for AiSettings {
    fn default() -> Self {
        let mut providers = std::collections::HashMap::new();
        for id in AiProviderId::ALL {
            providers.insert(id.key().to_string(), AiProviderConfig::for_provider(id));
        }
        Self {
            default_provider: AiProviderId::OpenAi,
            providers,
            system_prompt: Self::default_system_prompt(),
            custom_providers: Vec::new(),
            context: AiContextSettings::default(),
            agent: crate::models::AgentSettings::default(),
        }
    }
}

impl AiSettings {
    pub fn default_system_prompt() -> String {
        "你是一名专业的 Linux 运维与终端 Agent，可以直接查看和操作用户当前已连接的 SSH 终端。请遵循以下原则：\n\
1. 回复尽量精炼，优先通过工具收集事实再下结论，避免凭空猜测服务器状态；\n\
2. 需要执行危险或不可逆操作（如 rm/mkfs/dd/reboot/修改关键配置）前，先简要说明影响与回滚思路；\n\
3. 排查信息或只读检查优先使用后台 exec，避免打乱用户当前终端画面；需要用户直观看到过程或依赖当前目录/环境变量时再在可见终端执行；\n\
4. 所有展示给用户的命令使用 Markdown 代码块包裹并标注语言；\n\
5. 中文提问用中文回答，英文提问用英文回答。"
            .to_string()
    }

    pub fn get(&self, id: AiProviderId) -> AiProviderConfig {
        self.providers
            .get(id.key())
            .cloned()
            .unwrap_or_else(|| AiProviderConfig::for_provider(id))
    }

    pub fn set(&mut self, id: AiProviderId, cfg: AiProviderConfig) {
        self.providers.insert(id.key().to_string(), cfg);
    }

    /// 返回已通过连通性测试的供应商列表
    pub fn verified_providers(&self) -> Vec<AiProviderId> {
        AiProviderId::ALL
            .into_iter()
            .filter(|id| {
                self.providers
                    .get(id.key())
                    .map(|c| {
                        c.verified && (!id.requires_api_key() || !c.api_key.trim().is_empty())
                    })
                    .unwrap_or(false)
            })
            .collect()
    }

    /// 按 id 取自定义供应商
    pub fn custom_get(&self, id: &str) -> Option<&CustomProvider> {
        self.custom_providers.iter().find(|c| c.id == id)
    }

    /// 所有已通过测试的供应商引用（内置 + 自定义）
    pub fn verified_provider_refs(&self) -> Vec<ProviderRef> {
        let mut refs: Vec<ProviderRef> = self
            .verified_providers()
            .into_iter()
            .map(ProviderRef::Builtin)
            .collect();
        for c in &self.custom_providers {
            if c.verified && (is_local_base_url(&c.base_url) || !c.api_key.trim().is_empty()) {
                refs.push(ProviderRef::Custom(c.id.clone()));
            }
        }
        refs
    }

    /// 把供应商引用解析为统一视图（base_url / model 已填默认值）
    pub fn resolve(&self, r: &ProviderRef) -> Option<ResolvedProvider> {
        match r {
            ProviderRef::Builtin(id) => {
                let cfg = self.get(*id);
                let base_url = if cfg.base_url.trim().is_empty() {
                    id.default_base_url().to_string()
                } else {
                    cfg.base_url.clone()
                };
                let format = cfg.effective_format(*id);
                let models = cfg.model_list();
                let model = if cfg.model.trim().is_empty() {
                    models
                        .first()
                        .cloned()
                        .unwrap_or_else(|| id.default_model().to_string())
                } else {
                    cfg.model.clone()
                };
                Some(ResolvedProvider {
                    format,
                    name: id.label().to_string(),
                    api_key: cfg.api_key,
                    base_url,
                    model,
                    models,
                    verified: cfg.verified,
                    requires_api_key: id.requires_api_key(),
                })
            }
            ProviderRef::Custom(id) => {
                let c = self.custom_get(id)?;
                let models = c.model_list();
                let model = if c.model.trim().is_empty() {
                    models.first().cloned().unwrap_or_default()
                } else {
                    c.model.clone()
                };
                let base_url = c.base_url.trim_end_matches('/').to_string();
                let requires_api_key = !is_local_base_url(&base_url);
                Some(ResolvedProvider {
                    format: c.format,
                    name: c.name.clone(),
                    api_key: c.api_key.clone(),
                    base_url,
                    model,
                    models,
                    verified: c.verified,
                    requires_api_key,
                })
            }
        }
    }
}

/// 自定义供应商
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomProvider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub format: ApiFormat,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    /// 当前/默认模型（= models 列表首项）
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub verified: bool,
}

impl CustomProvider {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            format: ApiFormat::default(),
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            models: Vec::new(),
            verified: false,
        }
    }

    /// 可选模型列表：models 非空则用之，否则回退单个 model
    pub fn model_list(&self) -> Vec<String> {
        if !self.models.is_empty() {
            self.models.clone()
        } else if !self.model.is_empty() {
            vec![self.model.clone()]
        } else {
            Vec::new()
        }
    }
}

/// 供应商引用：内置枚举 or 自定义 id
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProviderRef {
    Builtin(AiProviderId),
    Custom(String),
}

impl ProviderRef {
    /// 字母头像字符（自定义供应商用名称首字符）；内置返回 None（用品牌图标）
    pub fn avatar_char(name: &str) -> char {
        name.trim()
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?')
    }
}

/// 供应商统一视图（服务层 / 侧边栏消费）
#[derive(Clone, Debug)]
pub struct ResolvedProvider {
    pub format: ApiFormat,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub verified: bool,
    pub requires_api_key: bool,
}

// ======================== 主题设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum Language {
    #[default]
    Chinese,
    English,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::Chinese => "简体中文",
            Language::English => "English",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub language: Language,
    pub accent_color: String,
    pub ui_font_family: String,
    pub ui_font_size: u32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            language: Language::Chinese,
            accent_color: "#3b82f6".to_string(), // Blue
            ui_font_family: "system-ui".to_string(),
            ui_font_size: 14,
        }
    }
}

// ======================== 终端设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum BellStyle {
    #[default]
    None,
    Visual,
    Sound,
}

/// 远端 OSC 52 对系统剪贴板的访问权限。
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum Osc52Policy {
    /// 禁止远端读取和写入系统剪贴板。
    Disabled,
    /// 仅允许远端把选中文本写入系统剪贴板（安全默认值）。
    #[default]
    WriteOnly,
    /// 允许远端读写系统剪贴板。
    ReadWrite,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TerminalSettings {
    // 字体
    pub font_family: String,
    pub font_size: u32,
    pub line_height: f32,
    pub font_weight: FontWeight,
    pub ligatures: bool,
    // 配色
    pub color_scheme: String,
    pub foreground_color: String,
    pub background_color: String,
    pub cursor_color: String,
    pub selection_color: String,
    /// 客户端关键字高亮（error/IP/路径等，取色自当前主题 ANSI 调色板）
    pub keyword_highlight: bool,
    /// 连接后自动向远端 shell 注入 `alias ls='ls --color=auto'`（会话级，不改服务器文件）
    pub auto_color_ls: bool,
    // 显示
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub cursor_blink_interval_ms: u32,
    pub background_opacity: u32,
    pub scrollback_lines: u32,
    pub padding: u32,
    // 行为
    pub copy_on_select: bool,
    pub right_click_paste: bool,
    pub trim_trailing_whitespace: bool,
    pub scroll_on_output: bool,
    pub scroll_multiplier: f32,
    pub bell_style: BellStyle,
    pub word_separators: String,
    pub osc52_policy: Osc52Policy,
    // Shell
    pub term_type: String,
    pub default_shell: String,
    pub shell_args: String,
    // 界面
    pub show_tab_bar: bool,
    pub show_command_input: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            line_height: 1.2,
            font_weight: FontWeight::Normal,
            ligatures: true,
            color_scheme: "One Dark".to_string(),
            foreground_color: "#abb2bf".to_string(),
            background_color: "#282c34".to_string(),
            cursor_color: "#528bff".to_string(),
            selection_color: "#3e4451".to_string(),
            keyword_highlight: true,
            auto_color_ls: false,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            cursor_blink_interval_ms: 500,
            background_opacity: 100,
            scrollback_lines: 10000,
            padding: 8,
            copy_on_select: false,
            right_click_paste: true,
            trim_trailing_whitespace: true,
            scroll_on_output: true,
            scroll_multiplier: 1.0,
            bell_style: BellStyle::None,
            word_separators: " <>()\"':;,│".to_string(),
            osc52_policy: Osc52Policy::WriteOnly,
            term_type: "xterm-256color".to_string(),
            default_shell: String::new(), // Use system default
            shell_args: String::new(),
            show_tab_bar: true,
            show_command_input: true,
        }
    }
}

// ======================== SFTP 设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SftpViewMode {
    #[default]
    List,
    Icons,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SftpSortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ConflictAction {
    #[default]
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SftpSettings {
    // 文件显示
    pub default_view_mode: SftpViewMode,
    pub show_hidden_files: bool,
    pub show_extensions: bool,
    pub sort_by: SftpSortBy,
    pub sort_ascending: bool,
    pub folders_first: bool,
    // 传输
    pub concurrent_transfers: u32,
    pub conflict_action: ConflictAction,
    pub preserve_timestamps: bool,
    pub speed_limit_kb: u32,
    pub resume_transfers: bool,
    pub open_folder_after_download: bool,
    // 路径
    pub local_default_path: String,
    pub remote_default_path: String,
    pub remember_last_path: bool,
    // 编辑器 - 基本设置
    pub use_builtin_editor: bool,
    pub external_editor_path: String,
    pub max_edit_file_size_kb: u32, // 最大可编辑文件大小 (KB)
    // 编辑器 - 内置编辑器外观
    pub editor_font_family: String,
    pub editor_font_size: u32,
    pub editor_line_height: f32,
    pub editor_gutter_width: u32,
    pub editor_gutter_padding: u32,
}

impl Default for SftpSettings {
    fn default() -> Self {
        Self {
            default_view_mode: SftpViewMode::List,
            show_hidden_files: false,
            show_extensions: true,
            sort_by: SftpSortBy::Name,
            sort_ascending: true,
            folders_first: true,
            concurrent_transfers: 3,
            conflict_action: ConflictAction::Ask,
            preserve_timestamps: true,
            speed_limit_kb: 0,
            resume_transfers: true,
            open_folder_after_download: false,
            local_default_path: String::new(),
            remote_default_path: String::new(),
            remember_last_path: true,
            use_builtin_editor: true,
            external_editor_path: String::new(),
            max_edit_file_size_kb: 5120, // 5MB
            editor_font_family: "JetBrains Mono".to_string(),
            editor_font_size: 14,
            editor_line_height: 1.4,
            editor_gutter_width: 48,
            editor_gutter_padding: 8,
        }
    }
}

// ======================== 监控设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum MonitorInterval {
    Sec1,
    #[default]
    Sec2,
    Sec5,
    Sec10,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ChartStyle {
    #[default]
    Line,
    Area,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitorSettings {
    // 数据采集
    pub refresh_interval: MonitorInterval,
    pub history_retention_minutes: u32,
    pub auto_deploy_agent: bool,
    // 显示
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disk: bool,
    pub show_network: bool,
    pub show_processes: bool,
    pub chart_style: ChartStyle,
    pub show_grid: bool,
    pub temperature_unit: TemperatureUnit,
    // 告警
    pub cpu_alert_threshold: u32,
    pub memory_alert_threshold: u32,
    pub disk_alert_threshold: u32,
    pub alert_notification: bool,
    pub alert_sound: bool,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            refresh_interval: MonitorInterval::Sec2,
            history_retention_minutes: 5,
            auto_deploy_agent: true,
            show_cpu: true,
            show_memory: true,
            show_disk: true,
            show_network: true,
            show_processes: true,
            chart_style: ChartStyle::Line,
            show_grid: true,
            temperature_unit: TemperatureUnit::Celsius,
            cpu_alert_threshold: 90,
            memory_alert_threshold: 85,
            disk_alert_threshold: 90,
            alert_notification: true,
            alert_sound: false,
        }
    }
}

// ======================== 连接设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum GlobalProxyType {
    #[default]
    None,
    Http,
    Socks4,
    Socks5,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionSettings {
    // SSH
    pub default_port: u16,
    pub connection_timeout_secs: u32,
    pub keepalive_interval_secs: u32,
    pub compression: bool,
    pub strict_host_key_checking: bool,
    // 自动重连
    pub auto_reconnect: bool,
    pub reconnect_attempts: u32,
    pub reconnect_interval_secs: u32,
    pub restore_session: bool,
    // 全局代理
    pub global_proxy_type: GlobalProxyType,
    pub global_proxy_host: String,
    pub global_proxy_port: u16,
    pub global_proxy_username: String,
    pub global_proxy_password: String,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            default_port: 22,
            connection_timeout_secs: 30,
            keepalive_interval_secs: 15,
            compression: false,
            strict_host_key_checking: false,
            auto_reconnect: true,
            reconnect_attempts: 3,
            reconnect_interval_secs: 5,
            restore_session: false,
            global_proxy_type: GlobalProxyType::None,
            global_proxy_host: String::new(),
            global_proxy_port: 0,
            global_proxy_username: String::new(),
            global_proxy_password: String::new(),
        }
    }
}

// ======================== 数据同步设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SyncMethod {
    #[default]
    None,
    WebDAV,
    ICloud,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum SyncInterval {
    #[default]
    Manual,
    OnStartup,
    Hourly,
    Daily,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum ConflictStrategy {
    #[default]
    Ask,
    LocalFirst,
    RemoteFirst,
    Merge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncSettings {
    pub enabled: bool,
    pub method: SyncMethod,
    pub auto_sync: bool,
    pub sync_interval: SyncInterval,
    // WebDAV
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_password: String,
    pub webdav_path: String,
    // 同步内容
    pub sync_servers: bool,
    pub sync_groups: bool,
    pub sync_settings: bool,
    pub sync_keybindings: bool,
    pub sync_keys: bool,
    // 冲突处理
    pub conflict_strategy: ConflictStrategy,
    pub backup_before_sync: bool,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            method: SyncMethod::None,
            auto_sync: false,
            sync_interval: SyncInterval::Manual,
            webdav_url: String::new(),
            webdav_username: String::new(),
            webdav_password: String::new(),
            webdav_path: "/shellmaster".to_string(),
            sync_servers: true,
            sync_groups: true,
            sync_settings: true,
            sync_keybindings: true,
            sync_keys: false,
            conflict_strategy: ConflictStrategy::Ask,
            backup_before_sync: true,
        }
    }
}

// ======================== 系统设置 ========================

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum AutoLockTime {
    #[default]
    Never,
    Min5,
    Min15,
    Hour1,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum HistoryRetention {
    #[default]
    Forever,
    Days7,
    Days30,
    OnExit,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemSettings {
    // 启动
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub restore_sessions: bool,
    pub check_updates: bool,
    #[serde(default)]
    pub auto_update: bool,
    // 窗口
    pub close_to_tray: bool,
    pub show_tray_icon: bool,
    pub single_instance: bool,
    pub save_window_position: bool,
    // 通知
    pub notify_on_connect: bool,
    pub notify_on_disconnect: bool,
    pub notify_on_transfer: bool,
    pub do_not_disturb: bool,
    // 隐私
    pub master_password_enabled: bool,
    pub auto_lock: AutoLockTime,
    pub history_retention: HistoryRetention,
    pub clear_clipboard_on_exit: bool,
    // 日志
    pub logging_enabled: bool,
    pub log_level: LogLevel,
    pub log_retention_days: u32,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized: false,
            restore_sessions: false,
            check_updates: true,
            auto_update: false,
            close_to_tray: false,
            show_tray_icon: true,
            single_instance: true,
            save_window_position: true,
            notify_on_connect: false,
            notify_on_disconnect: true,
            notify_on_transfer: true,
            do_not_disturb: false,
            master_password_enabled: false,
            auto_lock: AutoLockTime::Never,
            history_retention: HistoryRetention::Forever,
            clear_clipboard_on_exit: false,
            logging_enabled: true,
            log_level: LogLevel::Info,
            log_retention_days: 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_settings_accept_older_files_without_new_fields() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("osc52_policy");
        object.remove("cursor_blink_interval_ms");
        object.remove("scroll_multiplier");
        object.remove("padding");
        object.remove("term_type");
        object.remove("show_tab_bar");
        object.remove("show_command_input");
        object.remove("keyword_highlight");
        object.remove("auto_color_ls");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.osc52_policy, Osc52Policy::WriteOnly);
        assert_eq!(settings.cursor_blink_interval_ms, 500);
        assert_eq!(settings.padding, 8);
        assert_eq!(settings.term_type, "xterm-256color");
        assert!(settings.keyword_highlight);
        assert!(!settings.auto_color_ls);
    }

    fn legacy_settings(keepalive_secs: u32) -> AppSettings {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("settings_version");
        object["connection"]["keepalive_interval_secs"] = keepalive_secs.into();
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn migration_lowers_legacy_default_keepalive_once() {
        let mut settings = legacy_settings(60);
        assert_eq!(settings.settings_version, 0);
        assert!(settings.migrate());
        assert_eq!(settings.connection.keepalive_interval_secs, 15);
        assert_eq!(settings.settings_version, CURRENT_SETTINGS_VERSION);

        // 迁移后用户再改回 60 秒，不会被再次修改
        settings.connection.keepalive_interval_secs = 60;
        assert!(!settings.migrate());
        assert_eq!(settings.connection.keepalive_interval_secs, 60);
    }

    #[test]
    fn migration_keeps_custom_keepalive() {
        let mut settings = legacy_settings(30);
        assert!(settings.migrate());
        assert_eq!(settings.connection.keepalive_interval_secs, 30);
        assert!(!AppSettings::default().migrate());
    }

    #[test]
    fn system_settings_accept_older_files_without_auto_update() {
        let mut value = serde_json::to_value(SystemSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("auto_update");

        let settings: SystemSettings = serde_json::from_value(value).unwrap();
        assert!(settings.check_updates);
        assert!(!settings.auto_update);
    }

    #[test]
    fn ai_providers_support_coding_plan_presets_and_keyless_local_models() {
        let mut ai = AiSettings::default();
        assert_eq!(AiProviderId::ALL.len(), 20);
        assert_eq!(
            AiProviderId::SUBSCRIPTIONS.len()
                + AiProviderId::APIS.len()
                + AiProviderId::LOCALS.len(),
            AiProviderId::ALL.len()
        );
        assert_eq!(AiProviderId::ZhipuCoding.group(), AiProviderGroup::Subscription);
        assert_eq!(AiProviderId::OpenAi.group(), AiProviderGroup::Api);
        assert_eq!(AiProviderId::Ollama.group(), AiProviderGroup::Local);
        assert!(!AiProviderId::Ollama.requires_api_key());
        assert!(!AiProviderId::LmStudio.requires_api_key());
        assert!(AiProviderId::ZhipuCoding.requires_api_key());
        assert!(AiProviderId::ZhipuCoding.supports_format_override());
        assert!(AiProviderId::QwenCoding.supports_format_override());
        assert!(AiProviderId::KimiCoding.supports_format_override());
        assert!(AiProviderId::MiniMaxCoding.supports_format_override());
        assert!(AiProviderId::VolcengineCoding.supports_format_override());

        // Ollama 无需 API Key，只要 verified=true 即可进入可用供应商列表
        let mut ollama_cfg = ai.get(AiProviderId::Ollama);
        ollama_cfg.verified = true;
        ai.set(AiProviderId::Ollama, ollama_cfg);
        assert!(ai.verified_providers().contains(&AiProviderId::Ollama));

        let resolved_ollama = ai.resolve(&ProviderRef::Builtin(AiProviderId::Ollama)).unwrap();
        assert!(!resolved_ollama.requires_api_key);
        assert_eq!(resolved_ollama.base_url, "http://localhost:11434/v1");

        // 智谱 Coding Plan 切换到 Anthropic 协议后，resolve 应返回覆盖后的协议
        let mut zhipu_cfg = ai.get(AiProviderId::ZhipuCoding);
        zhipu_cfg.api_key = "test-key".into();
        zhipu_cfg.base_url = "https://open.bigmodel.cn/api/anthropic".into();
        zhipu_cfg.format = Some(ApiFormat::Anthropic);
        zhipu_cfg.verified = true;
        ai.set(AiProviderId::ZhipuCoding, zhipu_cfg);

        let resolved_zhipu = ai
            .resolve(&ProviderRef::Builtin(AiProviderId::ZhipuCoding))
            .unwrap();
        assert_eq!(resolved_zhipu.format, ApiFormat::Anthropic);
        assert!(resolved_zhipu.requires_api_key);

        assert!(is_local_base_url("http://localhost:11434/v1"));
        assert!(is_local_base_url("http://127.0.0.1:1234/v1"));
        assert!(!is_local_base_url("https://api.openai.com/v1"));
    }
}
