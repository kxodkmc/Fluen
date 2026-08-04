//! Markdown 文件的 YAML front matter 解析与序列化工具。
//!
//! 兼容 LF / CRLF 换行符，可被 [`loader`]（读取）与未来的
//! creator/save（写入）复用。
//!
//! ## 核心函数
//!
//! | 函数 | 用途 |
//! |------|------|
//! | [`split`] | 将文件内容分离为 `(yaml_str, body_str)` |
//! | [`parse_yaml`] | 从 YAML 字符串反序列化为指定类型 |
//! | [`join`] | 将 front matter 与正文组合为完整文件内容 |

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Front matter 分隔符。
const FM_DELIMITER: &str = "---";

/// Front matter 解析错误。
#[derive(Debug, thiserror::Error)]
pub enum FrontMatterError {
    /// 缺少起始标记 `---`。
    #[error("缺少 front matter 起始标记 ---")]
    MissingStart,
    /// 缺少结束标记 `---`。
    #[error("缺少 front matter 结束标记 ---")]
    MissingEnd,
    /// YAML 解析错误。
    #[error("YAML 解析错误: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// 将文件内容分离为 `(yaml_str, body_str)`。
///
/// 解析逻辑（CRLF 安全）：
/// 1. 去除可能的 BOM
/// 2. 按 `.lines()` 分割（自动处理 `\n` 和 `\r\n`）
/// 3. 检查首行 `trim() == "---"`
/// 4. 找到第二个 `trim() == "---"` 的行
/// 5. 中间部分为 YAML，剩余部分为正文
///
/// 若文件无 front matter，返回 [`FrontMatterError::MissingStart`]。
pub fn split(content: &str) -> Result<(String, String), FrontMatterError> {
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0].trim() != FM_DELIMITER {
        return Err(FrontMatterError::MissingStart);
    }

    let close_idx = lines[1..]
        .iter()
        .position(|line| line.trim() == FM_DELIMITER)
        .map(|i| i + 1)
        .ok_or(FrontMatterError::MissingEnd)?;

    let yaml = lines[1..close_idx].join("\n");
    let body = if close_idx + 1 < lines.len() {
        lines[close_idx + 1..].join("\n")
    } else {
        String::new()
    };

    Ok((yaml, body))
}

/// 从 YAML 字符串反序列化为指定类型。
pub fn parse_yaml<T: DeserializeOwned>(yaml: &str) -> Result<T, FrontMatterError> {
    serde_yaml::from_str(yaml).map_err(Into::into)
}

/// 将 front matter 与正文组合为完整文件内容。
///
/// 用于未来的保存 / 创建章节功能。
#[allow(dead_code)]
pub fn join<T: Serialize>(front_matter: &T, body: &str) -> Result<String, FrontMatterError> {
    let yaml = serde_yaml::to_string(front_matter)?;
    let yaml = yaml.trim_end();
    Ok(format!("---\n{}\n---\n{}", yaml, body))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lf() {
        let content = "---\ntitle: \"引言\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n# 引言\n正文";
        let (yaml, body) = split(content).unwrap();
        assert!(yaml.contains("title: \"引言\""));
        assert!(body.starts_with("# 引言"));
    }

    #[test]
    fn split_crlf() {
        let content = "---\r\ntitle: \"引言\"\r\ncreated: 2026-01-01T00:00:00Z\r\nupdated: 2026-01-01T00:00:00Z\r\n---\r\n# 引言\r\n正文\r\n";
        let (yaml, body) = split(content).unwrap();
        assert!(yaml.contains("title: \"引言\""));
        assert!(body.starts_with("# 引言"));
    }

    #[test]
    fn split_no_front_matter() {
        assert!(matches!(split("# 标题\n正文"), Err(FrontMatterError::MissingStart)));
    }

    #[test]
    fn split_unclosed() {
        assert!(matches!(
            split("---\ntitle: x\n# body"),
            Err(FrontMatterError::MissingEnd)
        ));
    }

    #[test]
    fn split_with_bom() {
        let content = "\u{FEFF}---\ntitle: x\n---\nbody";
        let (yaml, body) = split(content).unwrap();
        assert!(yaml.contains("title: x"));
        assert_eq!(body, "body");
    }

    #[test]
    fn split_empty_body() {
        let content = "---\ntitle: x\n---\n";
        let (_yaml, body) = split(content).unwrap();
        assert_eq!(body, "");
    }

    #[test]
    fn join_roundtrip() {
        #[derive(Serialize)]
        struct Fm {
            title: String,
            created: String,
        }
        let fm = Fm {
            title: "测试".into(),
            created: "2026-01-01T00:00:00Z".into(),
        };
        let joined = join(&fm, "# 测试\n正文").unwrap();
        let (yaml, body) = split(&joined).unwrap();
        assert!(yaml.contains("title: 测试"));
        assert!(body.starts_with("# 测试"));
    }
}
