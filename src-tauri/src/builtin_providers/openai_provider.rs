//! OpenAI 兼容的 Embedding 提供商。
//!
//! 支持任何遵循 OpenAI Embeddings API 格式（`POST /embeddings`）的端点，
//! 包括内置的 ModelScope 免费服务和用户自定义的第三方提供商。

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::embedding::BuiltinEmbeddingProvider;
use super::obfuscation::deobfuscate;

// ── ModelScope 内置预设 ──

/// ModelScope Embedding API 端点。
const MODELSCOPE_BASE_URL: &str = "https://ms-ens-da2b85a2-f12a.api-inference.modelscope.cn/v1";
/// ModelScope 免费推理 Token（混淆存储，运行时解密）。
const MODELSCOPE_API_KEY_CIPHER: &str =
    "087cc9991899369903127d58999599c61c96694908183801cc904c0990991dcdd88893899c9398";
/// ModelScope 默认模型 ID。
const MODELSCOPE_MODEL: &str = "Qwen/Qwen3-Embedding-4B";

/// 单批最大文本数（避免请求体过大）。
const MAX_BATCH_SIZE: usize = 32;

/// 单次 Embedding 请求超时（知识库写入持锁调用本接口，超时须远小于工具执行超时）。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// 带超时的 HTTP client；构建失败时回退默认 client。
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
}

/// OpenAI 兼容的 Embedding 提供商。
///
/// 内置 ModelScope 预设和用户自定义提供商均使用此结构，
/// 通过 `name` / `priority` / `base_url` / `api_key` / `model` 参数区分。
pub struct OpenAiEmbeddingProvider {
    name: String,
    priority: u32,
    base_url: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiEmbeddingProvider {
    /// 创建自定义提供商。
    pub fn new(
        name: impl Into<String>,
        priority: u32,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            priority,
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            client: http_client(),
        }
    }

    /// 创建内置 ModelScope 免费提供商（混淆密钥运行时解密）。
    pub fn modelscope() -> Self {
        Self::new(
            "modelscope-qwen3-embedding-4b",
            100,
            MODELSCOPE_BASE_URL,
            deobfuscate(MODELSCOPE_API_KEY_CIPHER),
            MODELSCOPE_MODEL,
        )
    }

    /// 拆分为多个批次（每批最多 `MAX_BATCH_SIZE` 条）。
    fn chunk_texts(texts: Vec<String>) -> Vec<Vec<String>> {
        if texts.len() <= MAX_BATCH_SIZE {
            return vec![texts];
        }
        texts
            .chunks(MAX_BATCH_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
}

// ── 请求 / 响应结构 ──

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    encoding_format: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
    #[allow(dead_code)]
    index: u32,
}

#[async_trait]
impl BuiltinEmbeddingProvider for OpenAiEmbeddingProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/embeddings", self.base_url);
        let batches = Self::chunk_texts(texts);
        let mut all_embeddings: Vec<Vec<f32>> = Vec::new();

        for batch in batches {
            let req = EmbeddingRequest {
                model: &self.model,
                input: &batch,
                encoding_format: "float",
            };

            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&req)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Embedding 请求失败: {e}"))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "Embedding API 返回错误 {status}: {body}"
                ));
            }

            let result: EmbeddingResponse = resp
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Embedding 响应解析失败: {e}"))?;

            // 按 index 排序后收集（API 可能不保证顺序）
            let mut items = result.data;
            items.sort_by_key(|item| item.index);
            for item in items {
                all_embeddings.push(item.embedding);
            }
        }

        Ok(all_embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_texts_single_batch() {
        let texts = vec!["a".to_string(), "b".to_string()];
        let chunks = OpenAiEmbeddingProvider::chunk_texts(texts);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 2);
    }

    #[test]
    fn chunk_texts_large_batch() {
        let texts: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        let chunks = OpenAiEmbeddingProvider::chunk_texts(texts);
        // 100 / 32 = 4 batches (32 + 32 + 32 + 4)
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].len(), 32);
        assert_eq!(chunks[3].len(), 4);
    }

    #[test]
    fn chunk_texts_empty() {
        let chunks = OpenAiEmbeddingProvider::chunk_texts(vec![]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_empty());
    }

    #[test]
    fn modelscope_provider_config() {
        let p = OpenAiEmbeddingProvider::modelscope();
        assert_eq!(p.name, "modelscope-qwen3-embedding-4b");
        assert_eq!(p.priority, 100);
        assert!(p.api_key.starts_with("ms-"));
        assert_eq!(p.model, MODELSCOPE_MODEL);
    }

    #[test]
    fn custom_provider_config() {
        let p = OpenAiEmbeddingProvider::new(
            "custom",
            50,
            "https://api.openai.com/v1",
            "sk-test",
            "text-embedding-3-small",
        );
        assert_eq!(p.name, "custom");
        assert_eq!(p.priority, 50);
        assert_eq!(p.api_key, "sk-test");
    }
}
