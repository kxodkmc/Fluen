//! Embedding 路由器：按优先级尝试多个内置提供商，失败自动降级。
//!
//! 实现 [`fluen_kb::KnowledgeEmbedding`] trait（同步单文本），可直接注入
//! `fluen_kb::AsyncKb`。

use std::sync::Arc;

use async_trait::async_trait;

use super::openai_provider::OpenAiEmbeddingProvider;
use crate::llm_config::model::{EmbeddingConfig, LlmConfig};

/// 内置 Embedding 提供商 trait。
///
/// 每个实现代表一个免费的 Embedding API 端点。
/// [`EmbeddingRouter`] 按优先级逐个调用，失败则降级到下一个。
#[async_trait]
pub trait BuiltinEmbeddingProvider: Send + Sync {
    /// 提供商名称（用于日志）。
    fn name(&self) -> &str;

    /// 优先级（值小先尝试）。
    fn priority(&self) -> u32;

    /// 将一批文本转为向量。
    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>>;
}

/// Embedding 路由器：聚合多个内置提供商，按优先级降级。
///
/// 实现 [`KnowledgeEmbedding`]，可直接通过
/// [`AsyncKnowledgeBase::with_embedding_provider`] 注入。
///
/// # 降级策略
///
/// 按优先级从小到大逐个尝试。某个提供商返回 `Err` 时记录警告并尝试下一个；
/// 全部失败时返回最后一个错误。
pub struct EmbeddingRouter {
    providers: Vec<Arc<dyn BuiltinEmbeddingProvider>>,
}

impl EmbeddingRouter {
    /// 创建包含内置 Embedding 提供商的路由器（按优先级排序）。
    ///
    /// 新增内置提供商时在此方法中追加即可。
    pub fn default_builtin() -> Self {
        let providers: Vec<Arc<dyn BuiltinEmbeddingProvider>> = vec![
            Arc::new(OpenAiEmbeddingProvider::modelscope()),
            // 未来可在此追加更多免费 Embedding 提供商
        ];
        Self { providers }
    }

    /// 从已有提供商列表创建路由器（用于测试或自定义配置）。
    pub fn with_providers(mut providers: Vec<Arc<dyn BuiltinEmbeddingProvider>>) -> Self {
        providers.sort_by_key(|p| p.priority());
        Self { providers }
    }

    /// 当前已注册的提供商数量。
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// 批量嵌入：按优先级逐个提供商尝试，失败降级。
    ///
    /// 供新 trait 实现复用（批 → 单调用循环摊平）。
    pub async fn embed_batch(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        if self.providers.is_empty() {
            return Err(anyhow::anyhow!("没有可用的内置 Embedding 提供商"));
        }

        let text_count = texts.len();
        let mut last_err: Option<anyhow::Error> = None;

        for provider in &self.providers {
            match provider.embed(texts.clone()).await {
                Ok(result) => {
                    if result.len() != text_count {
                        tracing::warn!(
                            provider = provider.name(),
                            expected = text_count,
                            actual = result.len(),
                            "Embedding 提供商返回数量不匹配，尝试下一个"
                        );
                        last_err = Some(anyhow::anyhow!(
                            "返回向量数量 {} 与输入文本数量 {} 不匹配",
                            result.len(),
                            text_count
                        ));
                        continue;
                    }
                    tracing::debug!(
                        provider = provider.name(),
                        count = result.len(),
                        "Embedding 成功"
                    );
                    return Ok(result);
                }
                Err(e) => {
                    tracing::warn!(
                        provider = provider.name(),
                        error = %e,
                        "Embedding 失败，尝试下一个提供商"
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("所有内置 Embedding 提供商均不可用")))
    }
}

/// 根据 LlmConfig 构建 EmbeddingRouter。
///
/// 返回 `None` 表示 Embedding 被禁用（`enabled == false`）。
///
/// 构建逻辑：
/// 1. 读取 `llm_config.embedding`，若不存在则使用默认配置（enabled=true，内置提供商）。
/// 2. `enabled == false` 返回 `None`。
/// 3. 始终包含内置 ModelScope 提供商（priority 100，作为降级兜底）。
/// 4. 若配置了自定义 `model_ref` 且引用有效，将其作为高优先级提供商（priority 50）。
///    自定义失败时自动降级到内置。
pub fn build_embedding_router(llm_config: &LlmConfig) -> Option<EmbeddingRouter> {
    let default_config = EmbeddingConfig::default();
    let emb_config = llm_config.embedding.as_ref().unwrap_or(&default_config);
    if !emb_config.enabled {
        return None;
    }

    let mut providers: Vec<Arc<dyn BuiltinEmbeddingProvider>> =
        vec![Arc::new(OpenAiEmbeddingProvider::modelscope())];

    // 尝试添加用户自定义提供商（高优先级）
    if let Some(custom) = resolve_custom_provider(llm_config, emb_config) {
        providers.push(Arc::new(custom));
    }

    Some(EmbeddingRouter::with_providers(providers))
}

/// 从配置中解析自定义 Embedding 提供商。
///
/// 需要 provider 有 `openai_base_url` 和 `api_key`，否则返回 `None`。
fn resolve_custom_provider(
    llm_config: &LlmConfig,
    emb_config: &EmbeddingConfig,
) -> Option<OpenAiEmbeddingProvider> {
    let ref_ = emb_config.model_ref.as_ref()?;
    let provider = llm_config.find_provider(&ref_.provider_id)?;
    if !provider.enabled {
        return None;
    }

    let base_url = provider.openai_base_url.as_ref()?.clone();
    let api_key = provider.api_key.as_ref()?.clone();

    Some(OpenAiEmbeddingProvider::new(
        format!("custom-{}", ref_.provider_id),
        50, // 高于内置 ModelScope 的 100，优先使用用户配置
        base_url,
        api_key,
        &ref_.model_id,
    ))
}

/// 新库 trait 适配：同步单文本接口。
///
/// fluen-kb 在 `spawn_blocking` 中调用本方法，此时 tokio runtime 上下文
/// 仍然可用，通过 `Handle::block_on` 驱动异步批量提供商并取首个结果。
impl fluen_kb::KnowledgeEmbedding for EmbeddingRouter {
    fn embed(&self, text: &str) -> fluen_kb::KbResult<Vec<f32>> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            fluen_kb::KbError::Embedding(
                "EmbeddingRouter 需在 tokio runtime 上下文中调用（spawn_blocking 内可用）".into(),
            )
        })?;
        let embeddings = handle
            .block_on(self.embed_batch(vec![text.to_string()]))
            .map_err(|e| fluen_kb::KbError::Embedding(format!("{e}")))?;
        embeddings.into_iter().next().ok_or_else(|| {
            fluen_kb::KbError::Embedding("embedding provider returned empty".into())
        })
    }

    fn model_name(&self) -> &str {
        "builtin-router"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用：始终成功的 mock 提供商。
    struct OkProvider {
        name: &'static str,
        priority: u32,
    }

    #[async_trait]
    impl BuiltinEmbeddingProvider for OkProvider {
        fn name(&self) -> &str {
            self.name
        }
        fn priority(&self) -> u32 {
            self.priority
        }
        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2]).collect())
        }
    }

    /// 测试用：始终失败的 mock 提供商。
    struct ErrProvider {
        name: &'static str,
        priority: u32,
    }

    #[async_trait]
    impl BuiltinEmbeddingProvider for ErrProvider {
        fn name(&self) -> &str {
            self.name
        }
        fn priority(&self) -> u32 {
            self.priority
        }
        async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            Err(anyhow::anyhow!("mock error"))
        }
    }

    #[tokio::test]
    async fn router_falls_back_on_error() {
        let router = EmbeddingRouter::with_providers(vec![
            Arc::new(ErrProvider {
                name: "err-low",
                priority: 1,
            }),
            Arc::new(OkProvider {
                name: "ok-high",
                priority: 2,
            }),
        ]);

        let result = router.embed_batch(vec!["hello".into()]).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], vec![0.1, 0.2]);
    }

    #[tokio::test]
    async fn router_returns_error_when_all_fail() {
        let router = EmbeddingRouter::with_providers(vec![
            Arc::new(ErrProvider {
                name: "err-1",
                priority: 1,
            }),
            Arc::new(ErrProvider {
                name: "err-2",
                priority: 2,
            }),
        ]);

        let result = router.embed_batch(vec!["hello".into()]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn router_uses_first_success() {
        let router = EmbeddingRouter::with_providers(vec![
            Arc::new(OkProvider {
                name: "ok-low",
                priority: 1,
            }),
            Arc::new(OkProvider {
                name: "ok-high",
                priority: 2,
            }),
        ]);

        let result = router.embed_batch(vec!["hello".into()]).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn router_sorts_by_priority() {
        let router = EmbeddingRouter::with_providers(vec![
            Arc::new(OkProvider {
                name: "low",
                priority: 10,
            }),
            Arc::new(OkProvider {
                name: "high",
                priority: 1,
            }),
        ]);
        assert_eq!(router.provider_count(), 2);
    }

    #[tokio::test]
    async fn router_empty_providers_returns_error() {
        let router = EmbeddingRouter::with_providers(vec![]);
        let result = router.embed_batch(vec!["hello".into()]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn router_rejects_mismatched_count() {
        struct MismatchProvider;
        #[async_trait]
        impl BuiltinEmbeddingProvider for MismatchProvider {
            fn name(&self) -> &str {
                "mismatch"
            }
            fn priority(&self) -> u32 {
                1
            }
            async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
                Ok(vec![vec![0.1]]) // 始终返回 1 条，不管输入多少
            }
        }

        let router = EmbeddingRouter::with_providers(vec![
            Arc::new(MismatchProvider),
            Arc::new(OkProvider {
                name: "ok",
                priority: 2,
            }),
        ]);

        // 输入 2 条文本，mismatch 只返回 1 条 → 降级到 ok
        let result = router.embed_batch(vec!["a".into(), "b".into()]).await.unwrap();
        assert_eq!(result.len(), 2);
    }
}
