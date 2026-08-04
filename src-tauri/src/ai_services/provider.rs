//! OCR 提供商统一接口与工厂。
//!
//! 通过 [`OcrProvider`] trait 解耦 importer 与具体 OCR 实现。
//! 新增 OCR 引擎时：实现 trait + 在 [`create_ocr_provider`] 工厂中添加分发分支。
//!
//! ## 扩展示例
//!
//! ```ignore
//! // 1. 实现 trait
//! #[async_trait]
//! impl OcrProvider for MyOcrClient { ... }
//!
//! // 2. 工厂分发
//! pub fn create_ocr_provider(provider: &AiServiceProvider) -> Result<Box<dyn OcrProvider>> {
//!     match provider.id.as_str() {
//!         "my-ocr" => Ok(Box::new(MyOcrClient::from_provider(provider)?)),
//!         _ => Ok(Box::new(PaddleOcrClient::from_provider(provider)?)),
//!     }
//! }
//! ```

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::error::AiServiceError;
use super::model::{AiServiceProvider, ServiceCategory};
use super::paddleocr::{OcrProgress, OcrResult, PaddleOcrClient};

/// OCR 提供商统一接口。
///
/// importer 依赖此 trait 而非具体类型，便于切换 / 新增 OCR 引擎。
#[async_trait]
pub trait OcrProvider: Send + Sync {
    /// 执行 OCR 识别。
    ///
    /// - `file_path`：本地文件路径或 URL（仅部分 provider 支持 URL）
    /// - `on_progress`：进度回调（任务状态变化时调用）
    /// - `cancel_token`：取消令牌，取消时返回 [`AiServiceError::Cancelled`]
    async fn recognize(
        &self,
        file_path: &str,
        on_progress: Box<dyn Fn(OcrProgress) + Send + Sync>,
        cancel_token: &CancellationToken,
    ) -> Result<OcrResult, AiServiceError>;
}

/// 工厂函数——根据提供商配置创建对应 [`OcrProvider`] 实现。
///
/// 当前仅支持 PaddleOCR。未来新增 OCR 引擎在此添加 match 分支。
pub fn create_ocr_provider(provider: &AiServiceProvider) -> Result<Box<dyn OcrProvider>, AiServiceError> {
    if provider.category != ServiceCategory::Ocr {
        return Err(AiServiceError::Other(format!(
            "提供商 {} 不是 OCR 类型",
            provider.id
        )));
    }
    Ok(Box::new(PaddleOcrClient::from_provider(provider)?))
}
