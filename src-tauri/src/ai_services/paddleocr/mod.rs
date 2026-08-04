//! # paddleocr
//!
//! PaddleOCR API 客户端——支持 Job（异步轮询）与 Sync（同步 base64）两种调用模式。
//!
//! ## 调用流程
//!
//! ### Job 模式（默认，适合大文件）
//!
//! 1. 提交任务（本地文件用 multipart，URL 用 JSON）→ 获取 `jobId`
//! 2. 轮询任务状态（每 3 秒），通过回调推送进度
//! 3. 任务完成后下载 JSONL 结果
//! 4. 解析每页 markdown 与图片，图片保存到临时目录
//!
//! ### Sync 模式（适合小文件，即时返回）
//!
//! 1. 读取文件并 base64 编码
//! 2. POST 到 API，直接获取结果
//! 3. 下载图片到临时目录
//!
//! ## 错误码映射
//!
//! | 状态码 | 说明 | 处理 |
//! |--------|------|------|
//! | 403 | Token 错误 | 检查 Token / URL 匹配 |
//! | 413 | 请求体过大 | 减少 PDF 页数 |
//! | 422 | 参数无效 | 参考 errorMsg |
//! | 429 | 超出单日解析上限 | 更换模型或稍后重试 |
//! | 500 | 服务器内部错误 | 联系 PaddleOCR |
//! | 503 | 请求过多 | 稍后重试 |
//! | 504 | 网关超时 | 稍后重试 |

pub mod client;
pub mod types;

pub use client::PaddleOcrClient;
#[allow(unused_imports)]
pub use types::{
    OcrApiMode, OcrImage, OcrPage, OcrProgress, OcrResult, PaddleOcrConfig, PaddleOcrOptions,
};
