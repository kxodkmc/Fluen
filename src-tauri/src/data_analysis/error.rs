//! 数据分析模块错误类型。

use serde::Serialize;
use socstat::error::SocStatError;

/// 数据分析统一错误。
#[derive(Debug, thiserror::Error)]
pub enum DataAnalysisError {
    #[error("统计数据错误: {0}")]
    SocStat(#[from] SocStatError),

    #[error("无法读取数据文件: {0}")]
    Io(#[from] std::io::Error),

    #[error("数据文件不存在: {0}")]
    NotFound(String),

    #[error("无效的输入: {0}")]
    InvalidInput(String),
}

impl Serialize for DataAnalysisError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}