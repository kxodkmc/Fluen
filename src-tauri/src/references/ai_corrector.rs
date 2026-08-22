//! AI 格式校正器——将 OCR 结果 / PDF 文本交给 LLM 校正为标准 Markdown。
//!
//! 模式 2（OCR + AI 校正）与模式 3（纯 AI 识别）的核心组件。
//!
//! ## 校正规则
//!
//! - AI 仅做格式纠错（标题层级、列表、表格、段落分隔），**不改动正文内容**
//! - 图片标签（HTML `<div><img.../></div>`）原样保留，AI 不接收图片文件
//! - OCR 结果与 PDF 文本冲突时，**以 PDF 文本为准**
//! - 输出含 YAML frontmatter（title / authors / journal / year）
//!
//! ## 依赖
//!
//! 复用 [`crate::llm_chat::build_llm_provider`] 构建 LLMProvider，
//! 避免重复 LLM 客户端装配代码。

use std::sync::Arc;

use referee_ai::provider::{
    ChatRequest, LLMProvider, Message, MessageContent, ThinkingConfig, ToolChoice,
};
use tokio_util::sync::CancellationToken;

use crate::llm_chat;
use crate::llm_config::model::LlmConfig;

use super::error::ReferenceError;
use super::import_mode::ReferenceImportMode;

/// AI 校正的最大输出 token 数（足够容纳典型论文）。
const MAX_OUTPUT_TOKENS: usize = 16384;
/// 采样温度——低温度保证格式稳定。
const TEMPERATURE: f32 = 0.1;

/// 文献 AI 格式校正器。
///
/// 持有 [`LLMProvider`] 与模型 ID，通过 [`AiCorrector::correct`] 发送
/// 校正请求并返回 AI 输出的标准 Markdown（含 frontmatter）。
pub struct AiCorrector {
    provider: Arc<dyn LLMProvider>,
    model_id: String,
    /// 模型是否支持思考模式（由模型能力标志决定，开启时请求注入 thinking 参数）。
    thinking: bool,
}

impl AiCorrector {
    /// 从 LLM 配置创建校正器。
    ///
    /// 解析 `scene_models.reference_import` 场景模型（未配置则回退到全局激活项），
    /// 构建对应的 LLMProvider。
    ///
    /// # 错误
    /// - [`ReferenceError::AiCorrection`]：未配置 LLM 提供商 / 场景模型引用失效 / api_key 缺失
    pub fn from_llm_config(llm: &LlmConfig) -> Result<Self, ReferenceError> {
        let (provider, model_id) = llm.resolve_reference_import().ok_or_else(|| {
            let msg = "未配置 LLM 提供商，请在设置中配置 AI 校正模型".to_string();
            tracing::error!(scope = "references", error = %msg, "创建 AI 校正器失败");
            ReferenceError::AiCorrection(msg)
        })?;

        let llm_provider = llm_chat::build_llm_provider(provider, &model_id).map_err(|e| {
            tracing::error!(scope = "references", error = %e, "构建 LLMProvider 失败");
            ReferenceError::AiCorrection(e.to_string())
        })?;

        tracing::info!(
            scope = "references",
            provider_id = %provider.id,
            model_id = %model_id,
            "AI 校正器已创建"
        );

        Ok(Self {
            provider: llm_provider,
            thinking: provider.model_supports_thinking(&model_id),
            model_id,
        })
    }

    /// 调用 AI 校正格式。
    ///
    /// - 模式 2：`ocr_md` 为重写图片路径后的 OCR 结果，`pdf_text` 为 PDF 提取文本
    /// - 模式 3：`ocr_md` 为 `None`，`pdf_text` 为 PDF 提取文本
    ///
    /// 返回 AI 输出的完整 Markdown（含 frontmatter）。
    ///
    /// # 取消
    /// 通过 `cancel_token` 响应取消信号，取消时返回 [`ReferenceError::Cancelled`]。
    pub async fn correct(
        &self,
        ocr_md: Option<&str>,
        pdf_text: &str,
        mode: ReferenceImportMode,
        cancel_token: &CancellationToken,
    ) -> Result<String, ReferenceError> {
        let system_prompt = build_system_prompt(mode);
        let user_content = build_user_content(ocr_md, pdf_text, mode);

        let req = ChatRequest {
            messages: vec![
                Message::system(MessageContent::text(system_prompt)),
                Message::user(MessageContent::text(user_content)),
            ],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            thinking: ThinkingConfig {
                enabled: self.thinking,
                effort: None,
            },
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: Some(TEMPERATURE),
            extra: std::collections::HashMap::new(),
        };

        tracing::info!(
            scope = "references",
            mode = ?mode,
            model = %self.model_id,
            ocr_chars = ocr_md.map(|s| s.chars().count()).unwrap_or(0),
            pdf_chars = pdf_text.chars().count(),
            "AI 校正请求发送"
        );

        // 通过 select! 响应取消信号
        let result = tokio::select! {
            r = self.provider.chat(req) => r,
            _ = cancel_token.cancelled() => {
                tracing::warn!(scope = "references", "AI 校正被取消");
                return Err(ReferenceError::Cancelled);
            }
        };

        let response = result.map_err(|e| {
            tracing::error!(scope = "references", error = %e, "AI 校正调用失败");
            ReferenceError::AiCorrection(e.to_string())
        })?;

        let content = response
            .message
            .content
            .as_text()
            .unwrap_or_default()
            .trim()
            .to_string();

        if content.is_empty() {
            return Err(ReferenceError::AiCorrection(
                "AI 返回空内容".into(),
            ));
        }

        tracing::info!(
            scope = "references",
            output_chars = content.chars().count(),
            usage = ?response.usage,
            "AI 校正完成"
        );

        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// 提示词构建
// ---------------------------------------------------------------------------

/// 角色描述（三种模式共用）。
const ROLE_PROMPT: &str =
    "你是一个学术文献格式校正助手。你的任务是将输入的文本校正为标准、完整的 Markdown 文件。";

/// 混合模式（OCR + AI 校正）校正规则——保留图片、以 PDF 文本为准。
const RULES_OCR_WITH_AI: &str = concat!(
    "\n## 校正规则\n",
    "1. **只做格式纠错**：校正标题层级（# 一级标题、## 二级标题等）、列表、表格、段落分隔、引用格式。不得增删、改写正文内容。\n",
    "2. **图片标签保留不变**：文本中出现的 HTML 图片标签（如 `<div style=\"text-align: center;\"><img src=\"...\" alt=\"Image\" .../></div>` 及其下方的图注 `<div style=\"text-align: center;\">图X ...</div>`）必须原样保留，不得修改路径、属性或图注文字。\n",
    "3. **冲突以 PDF 文本为准**：当 OCR 结果与 PDF 文本在文字内容上存在冲突（如 OCR 识别错误、多余空格、错字），以 PDF 文本为准。\n",
    "4. **保留分页**：不同页面之间用 `---` 分隔。\n",
    "5. **输出完整 Markdown**：直接输出校正后的完整 Markdown，不要添加任何解释说明。\n",
);

/// 混合模式（OCR + AI 校正）输入说明。
const INPUT_OCR_WITH_AI: &str = concat!(
    "\n## 输入说明\n",
    "本次为「OCR + AI 校正」模式。你将收到两部分内容：\n",
    "1. 【OCR 结果】——OCR 识别的 Markdown（可能含图片标签与排版噪声）\n",
    "2. 【PDF 文本】——从 PDF 直接提取的纯文本（无图片，文字准确但无格式）\n",
    "\n",
    "请综合两者，以 PDF 文本校正 OCR 的文字错误，以 OCR 结果保留图片标签与大致结构，输出标准 Markdown。",
);

/// 纯 AI 模式（AiOnly）校正规则——无图片资源，无 OCR 冲突处理。
const RULES_AI_ONLY: &str = concat!(
    "\n## 校正规则\n",
    "1. **只做格式纠错**：校正标题层级（# 一级标题、## 二级标题等）、列表、表格、段落分隔、引用格式。不得增删、改写正文内容。\n",
    "2. **保留分页**：不同页面之间用 `---` 分隔。\n",
    "3. **输出完整 Markdown**：直接输出校正后的完整 Markdown，不要添加任何解释说明。\n",
);

/// 纯 AI 模式（AiOnly）输入说明。
const INPUT_AI_ONLY: &str = concat!(
    "\n## 输入说明\n",
    "本次为「纯 AI 识别」模式。你将收到从 PDF 直接提取的纯文本（无图片、无格式）。",
    "请根据文本内容判断标题层级、段落、列表等结构，输出标准 Markdown。此模式无图片资源。",
);

/// 纯 OCR 模式校正规则——保留图片标签，无 PDF 文本冲突处理。
const RULES_OCR_ONLY: &str = concat!(
    "\n## 校正规则\n",
    "1. **只做格式纠错**：校正标题层级（# 一级标题、## 二级标题等）、列表、表格、段落分隔、引用格式。不得增删、改写正文内容。\n",
    "2. **图片标签保留不变**：文本中出现的 HTML 图片标签（如 `<div style=\"text-align: center;\"><img src=\"...\" alt=\"Image\" .../></div>` 及其下方的图注 `<div style=\"text-align: center;\">图X ...</div>`）必须原样保留，不得修改路径、属性或图注文字。\n",
    "3. **保留分页**：不同页面之间用 `---` 分隔。\n",
    "4. **输出完整 Markdown**：直接输出校正后的完整 Markdown，不要添加任何解释说明。\n",
);

/// 纯 OCR 模式输入说明。
const INPUT_OCR_ONLY: &str = concat!(
    "\n## 输入说明\n",
    "本次为「纯 OCR」模式。你将收到 OCR 识别的 Markdown 结果（可能含图片标签与排版噪声），请校正格式，输出标准 Markdown。",
);

/// Frontmatter 与输出格式说明（三种模式共用）。
const OUTPUT_TAIL: &str = concat!(
    "\n## Frontmatter\n",
    "在 Markdown 文件最开头输出 YAML frontmatter，包含以下字段（从正文识别，无法识别则省略该字段）：\n",
    "```yaml\n",
    "---\n",
    "title: 文献标题\n",
    "authors:\n",
    "  - 作者1\n",
    "  - 作者2\n",
    "journal: 期刊名（若可识别）\n",
    "year: '发表年份'（若可识别，用引号包裹）\n",
    "---\n",
    "```\n",
    "\n",
    "## 输出格式\n",
    "```\n",
    "---\n",
    "title: ...\n",
    "authors:\n",
    "  - ...\n",
    "---\n",
    "# 文献标题（正文首个一级标题）\n",
    "正文内容...\n",
    "```",
);

/// 构建系统提示词——按模式严格区分校正规则与输入说明。
fn build_system_prompt(mode: ReferenceImportMode) -> String {
    let (rules, input) = match mode {
        ReferenceImportMode::OcrWithAiCorrection => (RULES_OCR_WITH_AI, INPUT_OCR_WITH_AI),
        ReferenceImportMode::AiOnly => (RULES_AI_ONLY, INPUT_AI_ONLY),
        ReferenceImportMode::Ocr => (RULES_OCR_ONLY, INPUT_OCR_ONLY),
    };
    format!("{ROLE_PROMPT}{rules}{input}{OUTPUT_TAIL}")
}

/// 构建用户消息内容——根据模式组装 OCR 结果与 PDF 文本。
fn build_user_content(
    ocr_md: Option<&str>,
    pdf_text: &str,
    mode: ReferenceImportMode,
) -> String {
    match mode {
        ReferenceImportMode::OcrWithAiCorrection => {
            let ocr = ocr_md.unwrap_or("");
            format!(
                "【OCR 结果】\n{ocr}\n\n【PDF 文本】\n{pdf_text}\n\n请综合以上内容，输出校正后的标准 Markdown（含 frontmatter）。"
            )
        }
        ReferenceImportMode::AiOnly => {
            format!(
                "【PDF 文本】\n{pdf_text}\n\n请根据以上内容，输出校正后的标准 Markdown（含 frontmatter）。"
            )
        }
        ReferenceImportMode::Ocr => {
            // 不应到达此处，兜底处理
            let ocr = ocr_md.unwrap_or(pdf_text);
            format!("【OCR 结果】\n{ocr}\n\n请校正格式，输出标准 Markdown。")
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_config::model::ApiStyle;
    use std::collections::HashMap;

    #[test]
    fn system_prompt_contains_rules() {
        let prompt = build_system_prompt(ReferenceImportMode::OcrWithAiCorrection);
        assert!(prompt.contains("校正规则"));
        assert!(prompt.contains("图片标签保留不变"));
        assert!(prompt.contains("以 PDF 文本为准"));
        assert!(prompt.contains("Frontmatter"));
    }

    #[test]
    fn system_prompt_mode_specific() {
        let p2 = build_system_prompt(ReferenceImportMode::OcrWithAiCorrection);
        assert!(p2.contains("OCR + AI 校正"));

        let p3 = build_system_prompt(ReferenceImportMode::AiOnly);
        assert!(p3.contains("纯 AI 识别"));
        assert!(!p3.contains("OCR 结果"));
    }

    #[test]
    fn user_content_ocr_with_ai() {
        let content = build_user_content(
            Some("# OCR 标题\n![img](resource/ref-abc/imgs/x.jpg)"),
            "PDF 正文内容",
            ReferenceImportMode::OcrWithAiCorrection,
        );
        assert!(content.contains("【OCR 结果】"));
        assert!(content.contains("【PDF 文本】"));
        assert!(content.contains("OCR 标题"));
        assert!(content.contains("PDF 正文内容"));
    }

    #[test]
    fn user_content_ai_only() {
        let content = build_user_content(
            None,
            "PDF 纯文本",
            ReferenceImportMode::AiOnly,
        );
        assert!(content.contains("【PDF 文本】"));
        assert!(!content.contains("【OCR 结果】"));
        assert!(content.contains("PDF 纯文本"));
    }

    // ── AiCorrector 思考模式 ──────────────────────────────────────────

    fn provider(thinking: bool) -> crate::llm_config::model::ProviderConfig {
        use crate::llm_config::model::{
            ModelCapabilities, ModelConfig, ProviderConfig, ProviderType,
        };
        ProviderConfig {
            id: "zhipu".into(),
            name: "Zhipu".into(),
            provider_type: ProviderType::Custom,
            openai_base_url: Some("https://open.bigmodel.cn/api/paas/v4".into()),
            anthropic_base_url: None,
            api_key: Some("sk-test".into()),
            default_style: ApiStyle::OpenAI,
            extra_headers: HashMap::new(),
            enabled: true,
            models: vec![ModelConfig {
                id: "glm-5.2".into(),
                name: "GLM-5.2".into(),
                capabilities: ModelCapabilities {
                    thinking,
                    ..Default::default()
                },
                max_output_tokens: Some(4096),
                context_window: Some(1_000_000),
                description: None,
                enabled: true,
            }],
            created_at: None,
            updated_at: None,
        }
    }

    fn llm_with_reference_import(provider: crate::llm_config::model::ProviderConfig) -> LlmConfig {
        use crate::llm_config::model::{SceneModelRef, SceneModels};
        LlmConfig {
            providers: vec![provider],
            scene_models: Some(SceneModels {
                reference_import: Some(SceneModelRef {
                    provider_id: "zhipu".into(),
                    model_id: "glm-5.2".into(),
                }),
                ..SceneModels::default()
            }),
            ..LlmConfig::default()
        }
    }

    #[test]
    fn corrector_enables_thinking_for_thinking_model() {
        let llm = llm_with_reference_import(provider(true));
        let corrector = AiCorrector::from_llm_config(&llm).unwrap();
        assert!(corrector.thinking);
    }

    #[test]
    fn corrector_disables_thinking_for_non_thinking_model() {
        let llm = llm_with_reference_import(provider(false));
        let corrector = AiCorrector::from_llm_config(&llm).unwrap();
        assert!(!corrector.thinking);
    }
}
