//! PaddleOCR API 客户端。
//!
//! 封装与 PaddleOCR 服务的所有 HTTP 交互，支持 Job（异步轮询）与
//! Sync（同步 base64）两种调用模式。客户端与 Tauri 解耦，通过
//!回调函数推送进度，便于测试与复用。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::{engine::general_purpose, Engine};
use reqwest::multipart;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::super::error::AiServiceError;
use super::super::model::AiServiceProvider;
use super::types::{
    error_message_for_status, file_type_from_extension, OcrApiMode, OcrImage, OcrPage,
    OcrProgress, OcrResult, PaddleOcrConfig, PaddleOcrOptions,
};
use crate::platform::fluen_cache_dir;

use async_trait::async_trait;

/// 轮询间隔（秒）。
const POLL_INTERVAL_SECS: u64 = 3;

/// HTTP 请求超时（秒）。
const HTTP_TIMEOUT_SECS: u64 = 120;

// ---------------------------------------------------------------------------
// API 请求 payload（camelCase，与 PaddleOCR API 一致）
// ---------------------------------------------------------------------------

/// Job 模式的可选参数（嵌套在 `optionalPayload` 中）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiOptionalPayload {
    use_doc_orientation_classify: bool,
    use_doc_unwarping: bool,
    use_chart_recognition: bool,
}

impl From<&PaddleOcrOptions> for ApiOptionalPayload {
    fn from(opts: &PaddleOcrOptions) -> Self {
        Self {
            use_doc_orientation_classify: opts.use_doc_orientation_classify,
            use_doc_unwarping: opts.use_doc_unwarping,
            use_chart_recognition: opts.use_chart_recognition,
        }
    }
}

/// Sync 模式的完整请求体（可选参数平铺在顶层）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRequestBody {
    file: String,
    file_type: u32,
    #[serde(flatten)]
    options: ApiOptionalPayload,
}

/// Job 模式 URL 上传的请求体。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobUrlRequestBody {
    file_url: String,
    model: String,
    optional_payload: ApiOptionalPayload,
}

// ---------------------------------------------------------------------------
// PaddleOcrClient
// ---------------------------------------------------------------------------

/// PaddleOCR API 客户端。
///
/// 通过 [`PaddleOcrClient::from_provider`] 从配置创建，或通过
/// [`PaddleOcrClient::new`] 手动创建（用于测试）。
pub struct PaddleOcrClient {
    /// HTTP 客户端（复用连接池）。
    http: reqwest::Client,
    /// API 基础 URL（Job 模式为 jobs URL，Sync 模式为 API URL）。
    base_url: String,
    /// API Key / Token。
    api_key: String,
    /// 认证方案。
    auth_scheme: super::super::model::AuthScheme,
    /// 模型 ID（如 `PaddleOCR-VL-1.6`）。
    model: String,
    /// 提供商专属配置。
    config: PaddleOcrConfig,
}

impl PaddleOcrClient {
    /// 从提供商配置创建客户端。
    ///
    /// # 错误
    /// - 缺少 `api_base_url`
    /// - 缺少 `api_key`
    /// - 缺少模型配置
    pub fn from_provider(provider: &AiServiceProvider) -> Result<Self, AiServiceError> {
        let base_url = provider
            .api_base_url
            .clone()
            .ok_or_else(|| AiServiceError::Other("提供商缺少 api_base_url".into()))?;
        let api_key = provider
            .api_key
            .clone()
            .ok_or_else(|| AiServiceError::Other("提供商缺少 api_key，请在设置中配置".into()))?;
        let model = provider
            .active_model_id
            .clone()
            .or_else(|| provider.models.first().map(|m| m.id.clone()))
            .ok_or_else(|| AiServiceError::Other("提供商缺少模型配置".into()))?;
        let config = PaddleOcrConfig::from_json(provider.provider_config.as_ref());

        Ok(Self::new(
            base_url,
            api_key,
            provider.auth_scheme,
            model,
            config,
        ))
    }

    /// 手动创建客户端（主要用于测试）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: String,
        api_key: String,
        auth_scheme: super::super::model::AuthScheme,
        model: String,
        config: PaddleOcrConfig,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .expect("reqwest client build failed");
        Self {
            http,
            base_url,
            api_key,
            auth_scheme,
            model,
            config,
        }
    }

    /// 执行 OCR 识别。
    ///
    /// 根据 `config.api_mode` 分发到 Job 或 Sync 模式。
    /// `file_path` 为本地文件路径或以 `http` 开头的 URL（仅 Job 模式支持 URL）。
    ///
    /// # 进度回调
    ///
    /// `on_progress` 在任务状态变化时被调用，用于推送 Tauri 事件。
    ///
    /// # 取消
    ///
    /// `cancel_token` 在每次轮询前检查，取消时返回 [`AiServiceError::Cancelled`]。
    pub async fn recognize(
        &self,
        file_path: &str,
        on_progress: impl Fn(OcrProgress) + Send + Sync,
        cancel_token: &CancellationToken,
    ) -> Result<OcrResult, AiServiceError> {
        let temp_dir = create_temp_dir()?;

        match self.config.api_mode {
            OcrApiMode::Job => {
                self.recognize_job(file_path, &temp_dir, &on_progress, cancel_token)
                    .await
            }
            OcrApiMode::Sync => {
                self.recognize_sync(file_path, &temp_dir, &on_progress, cancel_token)
                    .await
            }
        }
    }

    // -----------------------------------------------------------------------
    // Job 模式
    // -----------------------------------------------------------------------

    /// Job 模式：提交任务 → 轮询 → 下载结果。
    async fn recognize_job(
        &self,
        file_path: &str,
        temp_dir: &Path,
        on_progress: &impl Fn(OcrProgress),
        cancel_token: &CancellationToken,
    ) -> Result<OcrResult, AiServiceError> {
        // 1. 提交任务
        let job_id = self.submit_job(file_path).await?;

        // 2. 轮询状态
        let json_url = loop {
            if cancel_token.is_cancelled() {
                return Err(AiServiceError::Cancelled);
            }

            let status = self.poll_job(&job_id).await?;

            on_progress(OcrProgress {
                state: status.data.state.clone(),
                total_pages: status
                    .data
                    .extract_progress
                    .as_ref()
                    .and_then(|p| p.total_pages),
                extracted_pages: status
                    .data
                    .extract_progress
                    .as_ref()
                    .and_then(|p| p.extracted_pages),
                message: None,
            });

            match status.data.state.as_str() {
                "done" => {
                    let url = status
                        .data
                        .result_url
                        .and_then(|r| r.json_url)
                        .ok_or_else(|| {
                            AiServiceError::ApiBusiness("结果中缺少 jsonUrl".into())
                        })?;
                    break url;
                }
                "failed" => {
                    let msg = status.data.error_msg.unwrap_or_else(|| "未知错误".into());
                    return Err(AiServiceError::ApiBusiness(format!("任务失败: {msg}")));
                }
                _ => {
                    // pending 或 running，继续轮询
                    tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            }
        };

        // 3. 下载并解析结果
        self.download_and_parse_results(&json_url, temp_dir).await
    }

    /// 提交 Job：本地文件用 multipart，URL 用 JSON。
    async fn submit_job(&self, file_path: &str) -> Result<String, AiServiceError> {
        let auth_header = self.auth_scheme.header_value(&self.api_key);
        let payload = ApiOptionalPayload::from(&self.config.options);

        if file_path.starts_with("http") {
            // URL 模式
            let body = JobUrlRequestBody {
                file_url: file_path.to_string(),
                model: self.model.clone(),
                optional_payload: payload,
            };
            let resp = self
                .http
                .post(&self.base_url)
                .header("Authorization", &auth_header)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            self.parse_job_submit_response(resp).await
        } else {
            // 本地文件模式
            let file_bytes = std::fs::read(file_path).map_err(|e| {
                AiServiceError::Other(format!("读取文件失败: {e}"))
            })?;
            let filename = Path::new(file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into());

            let optional_json = serde_json::to_string(&payload)?;

            let form = multipart::Form::new()
                .text("model", self.model.clone())
                .text("optionalPayload", optional_json)
                .part(
                    "file",
                    multipart::Part::bytes(file_bytes).file_name(filename),
                );

            let resp = self
                .http
                .post(&self.base_url)
                .header("Authorization", &auth_header)
                .multipart(form)
                .send()
                .await?;

            self.parse_job_submit_response(resp).await
        }
    }

    /// 解析 Job 提交响应。
    async fn parse_job_submit_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<String, AiServiceError> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_message_for_status(status.as_u16(), &body));
        }
        let parsed: super::types::JobSubmitResponse = resp.json().await?;
        Ok(parsed.data.job_id)
    }

    /// 轮询 Job 状态。
    async fn poll_job(
        &self,
        job_id: &str,
    ) -> Result<super::types::JobStatusResponse, AiServiceError> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), job_id);
        let auth_header = self.auth_scheme.header_value(&self.api_key);

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_message_for_status(status.as_u16(), &body));
        }

        let parsed: super::types::JobStatusResponse = resp.json().await?;
        Ok(parsed)
    }

    // -----------------------------------------------------------------------
    // Sync 模式
    // -----------------------------------------------------------------------

    /// Sync 模式：base64 上传 → 直接返回结果。
    async fn recognize_sync(
        &self,
        file_path: &str,
        temp_dir: &Path,
        on_progress: &impl Fn(OcrProgress),
        cancel_token: &CancellationToken,
    ) -> Result<OcrResult, AiServiceError> {
        if cancel_token.is_cancelled() {
            return Err(AiServiceError::Cancelled);
        }

        if file_path.starts_with("http") {
            return Err(AiServiceError::Other(
                "Sync 模式不支持 URL 输入，请使用 Job 模式".into(),
            ));
        }

        on_progress(OcrProgress {
            state: "running".into(),
            total_pages: None,
            extracted_pages: None,
            message: Some("正在上传文件...".into()),
        });

        // 读取并编码文件
        let file_bytes = std::fs::read(file_path)
            .map_err(|e| AiServiceError::Other(format!("读取文件失败: {e}")))?;
        let encoded = general_purpose::STANDARD.encode(&file_bytes);
        let file_type = file_type_from_extension(file_path);

        let body = SyncRequestBody {
            file: encoded,
            file_type,
            options: ApiOptionalPayload::from(&self.config.options),
        };

        let auth_header = self.auth_scheme.header_value(&self.api_key);
        let resp = self
            .http
            .post(&self.base_url)
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_message_for_status(status.as_u16(), &body));
        }

        on_progress(OcrProgress {
            state: "done".into(),
            total_pages: None,
            extracted_pages: None,
            message: Some("识别完成，正在处理结果...".into()),
        });

        // 解析响应（结构与 JSONL 单行相同）
        let raw: super::types::OcrRawResult = resp.json().await?;
        self.parse_raw_result(raw, temp_dir).await
    }

    // -----------------------------------------------------------------------
    // 结果解析与图片下载
    // -----------------------------------------------------------------------

    /// 下载 JSONL 结果并解析。
    async fn download_and_parse_results(
        &self,
        json_url: &str,
        temp_dir: &Path,
    ) -> Result<OcrResult, AiServiceError> {
        let resp = self.http.get(json_url).send().await?;
        let resp = resp.error_for_status()?;
        let text = resp.text().await?;

        let mut pages: Vec<OcrPage> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let raw: super::types::OcrRawResult = serde_json::from_str(line)?;
            let mut page_results = self.parse_raw_result_pages(&raw, temp_dir).await?;
            pages.append(&mut page_results);
        }

        Ok(OcrResult {
            pages,
            temp_dir: temp_dir.to_string_lossy().to_string(),
        })
    }

    /// 解析单个 `OcrRawResult`（Sync 模式使用）。
    async fn parse_raw_result(
        &self,
        raw: super::types::OcrRawResult,
        temp_dir: &Path,
    ) -> Result<OcrResult, AiServiceError> {
        let pages = self.parse_raw_result_pages(&raw, temp_dir).await?;
        Ok(OcrResult {
            pages,
            temp_dir: temp_dir.to_string_lossy().to_string(),
        })
    }

    /// 解析 `OcrRawResult` 中的所有页面，下载图片。
    async fn parse_raw_result_pages(
        &self,
        raw: &super::types::OcrRawResult,
        temp_dir: &Path,
    ) -> Result<Vec<OcrPage>, AiServiceError> {
        let mut pages = Vec::new();
        for (idx, res) in raw.result.layout_parsing_results.iter().enumerate() {
            let images = self
                .download_images(&res.markdown.images, temp_dir)
                .await?;
            pages.push(OcrPage {
                index: idx as u32,
                markdown: res.markdown.text.clone(),
                images,
            });
        }
        Ok(pages)
    }

    /// 下载所有图片到临时目录。
    ///
    /// 图片按 `markdown.images` 中的 key（相对路径）保存到 `temp_dir` 下。
    async fn download_images(
        &self,
        images: &HashMap<String, String>,
        temp_dir: &Path,
    ) -> Result<Vec<OcrImage>, AiServiceError> {
        let mut result = Vec::new();
        for (name, url) in images {
            let img_path = temp_dir.join(name);
            if let Some(parent) = img_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let resp = self.http.get(url).send().await?;
            if !resp.status().is_success() {
                return Err(AiServiceError::Other(format!(
                    "下载图片失败 ({}): {}",
                    resp.status(),
                    name
                )));
            }
            let bytes = resp.bytes().await?;
            std::fs::write(&img_path, &bytes)?;

            result.push(OcrImage {
                name: name.clone(),
                path: img_path.to_string_lossy().to_string(),
            });
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// OcrProvider trait 实现
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::ai_services::provider::OcrProvider for PaddleOcrClient {
    async fn recognize(
        &self,
        file_path: &str,
        on_progress: Box<dyn Fn(OcrProgress) + Send + Sync>,
        cancel_token: &CancellationToken,
    ) -> Result<OcrResult, AiServiceError> {
        // 复用已有的 recognize 方法
        self.recognize(file_path, on_progress, cancel_token).await
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 创建 OCR 临时目录（`{cache_dir}/ocr/{uuid}/`）。
fn create_temp_dir() -> Result<PathBuf, AiServiceError> {
    let cache_dir = fluen_cache_dir()?;
    let temp_dir = cache_dir.join("ocr").join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}
