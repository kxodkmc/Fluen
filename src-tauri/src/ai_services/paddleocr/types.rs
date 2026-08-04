//! PaddleOCR 的类型定义。
//!
//! 包含三类类型：
//! 1. 提供商专属配置（[`PaddleOcrConfig`]）——存储于 `provider_config` 字段
//! 2. API 请求 / 响应类型——与 PaddleOCR API 的 JSON 结构对应
//! 3. OCR 结果类型——返回给前端的结构化数据

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::super::error::AiServiceError;

// ---------------------------------------------------------------------------
// 提供商专属配置
// ---------------------------------------------------------------------------

/// PaddleOCR API 调用模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrApiMode {
    /// 异步 Job 模式（提交任务 → 轮询 → 下载结果）。
    #[default]
    Job,
    /// 同步模式（base64 上传，直接返回结果）。
    Sync,
}

/// PaddleOCR 识别选项。
///
/// 对应 API 的 `optionalPayload` 字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaddleOcrOptions {
    /// 文档方向分类。
    #[serde(default)]
    pub use_doc_orientation_classify: bool,
    /// 文档去畸变。
    #[serde(default)]
    pub use_doc_unwarping: bool,
    /// 图表识别。
    #[serde(default)]
    pub use_chart_recognition: bool,
}

impl Default for PaddleOcrOptions {
    fn default() -> Self {
        Self {
            use_doc_orientation_classify: false,
            use_doc_unwarping: false,
            use_chart_recognition: false,
        }
    }
}

/// PaddleOCR 提供商专属配置。
///
/// 序列化为 JSON 存储于 [`super::super::model::AiServiceProvider::provider_config`]。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaddleOcrConfig {
    /// API 调用模式。
    #[serde(default)]
    pub api_mode: OcrApiMode,
    /// 识别选项。
    #[serde(default)]
    pub options: PaddleOcrOptions,
}

impl PaddleOcrConfig {
    /// 从 `serde_json::Value` 解析配置。
    ///
    /// `None` 或解析失败时返回默认配置。
    pub fn from_json(value: Option<&serde_json::Value>) -> Self {
        match value {
            Some(v) => serde_json::from_value(v.clone()).unwrap_or_default(),
            None => Self::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// API 响应类型（camelCase，与 PaddleOCR API 一致）
// ---------------------------------------------------------------------------

/// Job 提交响应。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSubmitResponse {
    pub data: JobSubmitData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSubmitData {
    pub job_id: String,
}

/// Job 状态响应。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobStatusResponse {
    pub data: JobStatusData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobStatusData {
    pub state: String,
    #[serde(default)]
    pub extract_progress: Option<ExtractProgress>,
    #[serde(default)]
    pub error_msg: Option<String>,
    #[serde(default)]
    pub result_url: Option<ResultUrl>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtractProgress {
    #[serde(default)]
    pub total_pages: Option<u32>,
    #[serde(default)]
    pub extracted_pages: Option<u32>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultUrl {
    #[serde(default)]
    pub json_url: Option<String>,
}

/// JSONL 结果行 / Sync 响应（结构相同）。
#[derive(Debug, Deserialize)]
pub(crate) struct OcrRawResult {
    pub result: OcrRawResultData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OcrRawResultData {
    #[serde(default)]
    pub layout_parsing_results: Vec<LayoutParsingResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutParsingResult {
    pub markdown: MarkdownResult,
    #[serde(default)]
    pub output_images: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MarkdownResult {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub images: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// OCR 结果类型（返回给前端）
// ---------------------------------------------------------------------------

/// OCR 进度信息（通过事件推送到前端）。
#[derive(Debug, Clone, Serialize)]
pub struct OcrProgress {
    /// 任务状态：`"pending"` / `"running"` / `"done"` / `"failed"`。
    pub state: String,
    /// 总页数（`running` 状态下可能可用）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u32>,
    /// 已处理页数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_pages: Option<u32>,
    /// 附加消息（如错误信息）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// OCR 结果。
#[derive(Debug, Clone, Serialize)]
pub struct OcrResult {
    /// 识别结果（按页）。
    pub pages: Vec<OcrPage>,
    /// 图片保存的临时目录路径。
    ///
    /// 前端可从此目录读取图片文件，导入完成后可删除。
    pub temp_dir: String,
}

/// 单页 OCR 结果。
#[derive(Debug, Clone, Serialize)]
pub struct OcrPage {
    /// 页码索引（从 0 开始）。
    pub index: u32,
    /// Markdown 文本。
    pub markdown: String,
    /// 页面中引用的图片列表。
    pub images: Vec<OcrImage>,
}

/// OCR 结果中的图片。
#[derive(Debug, Clone, Serialize)]
pub struct OcrImage {
    /// 图片名称（markdown 中的引用路径，如 `images/doc_0_xxx.jpg`）。
    pub name: String,
    /// 本地保存路径。
    pub path: String,
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 根据 API 状态码返回用户友好的错误信息。
pub(crate) fn error_message_for_status(status: u16, body: &str) -> AiServiceError {
    let msg = match status {
        403 => "Token 错误，请检查 Token 是否正确或 URL 是否匹配",
        413 => "请求体过大，请减少 PDF 文件的页数或文件大小",
        422 => "参数无效，请参考 errorMsg 解决",
        429 => "超出单日解析最大页数，请使用其他模型或稍后再试",
        500 => "服务器内部错误，如频繁遇到请联系 PaddleOCR 官方人员",
        503 => "当前请求过多，请稍后再试",
        504 => "网关超时，请稍后再试",
        _ => "API 返回错误",
    };
    AiServiceError::ApiStatus {
        status,
        body: format!("{msg} | {body}"),
    }
}

/// 根据文件扩展名判断文件类型（Sync 模式需要）。
///
/// 返回 `0` 表示 PDF，`1` 表示图片。
pub(crate) fn file_type_from_extension(path: &str) -> u32 {
    let ext = path.rsplit('.').next().map(|e| e.to_lowercase());
    match ext.as_deref() {
        Some("pdf") => 0,
        Some("jpg") | Some("jpeg") | Some("png") | Some("bmp") | Some("tiff") | Some("tif")
        | Some("webp") => 1,
        _ => 0, // 默认按 PDF 处理
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paddle_ocr_config_default() {
        let config = PaddleOcrConfig::default();
        assert_eq!(config.api_mode, OcrApiMode::Job);
        assert!(!config.options.use_doc_orientation_classify);
    }

    #[test]
    fn paddle_ocr_config_from_json() {
        let json = serde_json::json!({
            "api_mode": "sync",
            "options": {
                "use_doc_orientation_classify": true,
                "use_doc_unwarping": false,
                "use_chart_recognition": true
            }
        });
        let config = PaddleOcrConfig::from_json(Some(&json));
        assert_eq!(config.api_mode, OcrApiMode::Sync);
        assert!(config.options.use_doc_orientation_classify);
        assert!(config.options.use_chart_recognition);
    }

    #[test]
    fn paddle_ocr_config_from_none() {
        let config = PaddleOcrConfig::from_json(None);
        assert_eq!(config.api_mode, OcrApiMode::Job);
    }

    #[test]
    fn paddle_ocr_config_from_invalid_json() {
        let json = serde_json::json!("not an object");
        let config = PaddleOcrConfig::from_json(Some(&json));
        assert_eq!(config.api_mode, OcrApiMode::Job);
    }

    #[test]
    fn file_type_detection() {
        assert_eq!(file_type_from_extension("test.pdf"), 0);
        assert_eq!(file_type_from_extension("test.jpg"), 1);
        assert_eq!(file_type_from_extension("test.PNG"), 1);
        assert_eq!(file_type_from_extension("test.unknown"), 0);
    }

    #[test]
    fn parse_job_submit_response() {
        let json = r#"{"data":{"jobId":"abc-123"}}"#;
        let resp: JobSubmitResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.job_id, "abc-123");
    }

    #[test]
    fn parse_job_status_done() {
        let json = r#"{"data":{"state":"done","extractProgress":{"totalPages":5,"extractedPages":5},"resultUrl":{"jsonUrl":"https://example.com/result.jsonl"}}}"#;
        let resp: JobStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.state, "done");
        assert_eq!(resp.data.extract_progress.unwrap().total_pages, Some(5));
        assert!(resp.data.result_url.unwrap().json_url.is_some());
    }

    #[test]
    fn parse_jsonl_line() {
        let json = r##"{"result":{"layoutParsingResults":[{"markdown":{"text":"# Title","images":{"images/fig1.jpg":"https://example.com/fig1.jpg"}},"outputImages":{}}]}}"##;
        let result: OcrRawResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.result.layout_parsing_results.len(), 1);
        assert_eq!(result.result.layout_parsing_results[0].markdown.text, "# Title");
        assert!(result.result.layout_parsing_results[0]
            .markdown
            .images
            .contains_key("images/fig1.jpg"));
    }
}
