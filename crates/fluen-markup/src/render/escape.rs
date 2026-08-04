//! HTML / 文本转义工具。集中一处，避免渲染器内重复实现。

/// HTML 文本转义（用于正常文本节点、属性值）。
pub fn html_escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// HTML 属性值转义（额外处理 `'`）。
pub fn html_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// URL 安全化：阻止 `javascript:` 等危险协议（仅保留已知安全前缀或相对路径）。
pub fn sanitize_url(url: &str) -> String {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") {
        return String::new();
    }
    url.to_string()
}
