//! 文件 IO 便利函数：读取文档、加载文献库、写出渲染结果。
//!
//! 文献库 JSON 格式（与项目 `references/references-index.json` 衔接，规范 §7）：
//! ```json
//! [
//!   { "id": "ref-a1b2c3d4", "authors": ["Chen"], "year": "2020", "title": "..." }
//! ]
//! ```
//! 也接受 `{"ref-a1b2c3d4": {...}}` 的 map 形式。

use std::collections::HashMap;
use std::path::Path;

use crate::context::{InMemoryReferences, ReferenceEntry, ReferenceProvider};
use crate::error::Result;
use crate::model::Document;

/// 从文件读取并解析为 [`Document`]。
pub fn read_document(path: &Path) -> Result<Document> {
    let text = std::fs::read_to_string(path)?;
    crate::parse::parse_document(&text)
}

/// 从 JSON 字符串构造内存文献库。兼容数组与对象两种形态。
pub fn references_from_json(json: &str) -> Result<InMemoryReferences> {
    let trimmed = json.trim();
    let mut store = InMemoryReferences::new();
    if trimmed.starts_with('[') {
        // 数组
        parse_array(trimmed, &mut store)?;
    } else if trimmed.starts_with('{') {
        // 对象（map 或单条）
        parse_object(trimmed, &mut store)?;
    } else {
        return Err(crate::error::MarkupError::Render { message: "references-index.json 应为 JSON 数组或对象".into() });
    }
    Ok(store)
}

/// 从文件加载文献库。
pub fn load_references(path: &Path) -> Result<InMemoryReferences> {
    let text = std::fs::read_to_string(path)?;
    references_from_json(&text)
}

fn parse_array(json: &str, store: &mut InMemoryReferences) -> Result<()> {
    // 手写极简 JSON 解析（避免引入 serde/serde_json 依赖）。
    // 仅支持本 crate 定义的结构：数组，元素为带 id/authors/year/title 的对象。
    let parser = JsonParser::new(json);
    let value = parser.parse()?;
    if let JsonValue::Array(items) = value {
        for item in items {
            if let JsonValue::Object(map) = item {
                let id = map.get("id").and_then(|v| v.as_string()).unwrap_or_default();
                if id.is_empty() { continue; }
                let entry = entry_from_map(&map);
                store.insert(id, entry);
            }
        }
    }
    Ok(())
}

fn parse_object(json: &str, store: &mut InMemoryReferences) -> Result<()> {
    let parser = JsonParser::new(json);
    let value = parser.parse()?;
    if let JsonValue::Object(map) = value {
        // 若含 "id" 字段，视为单条
        if map.contains_key("id") {
            let id = map.get("id").and_then(|v| v.as_string()).unwrap_or_default();
            if !id.is_empty() {
                store.insert(id, entry_from_map(&map));
            }
            return Ok(());
        }
        // 否则视为 id → entry 的 map
        for (id, v) in map {
            if let JsonValue::Object(inner) = v {
                store.insert(id, entry_from_map(&inner));
            }
        }
    }
    Ok(())
}

fn entry_from_map(map: &HashMap<String, JsonValue>) -> ReferenceEntry {
    let authors = match map.get("authors") {
        Some(JsonValue::Array(arr)) => arr.iter().filter_map(|v| v.as_string()).collect::<Vec<String>>(),
        Some(JsonValue::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    };
    let year = map.get("year").and_then(|v| v.as_string());
    let title = map.get("title").and_then(|v| v.as_string());
    ReferenceEntry { authors, year, title }
}

// ───────────────────────── 极简 JSON 解析（仅满足文献库需求）─────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

impl JsonValue {
    fn as_string(&self) -> Option<String> {
        if let JsonValue::String(s) = self { Some(s.clone()) } else { None }
    }
}

struct JsonParser<'a> { chars: Vec<char>, pos: usize, _src: &'a str }
impl<'a> JsonParser<'a> {
    fn new(src: &'a str) -> Self { Self { chars: src.chars().collect(), pos: 0, _src: src } }

    fn parse(mut self) -> Result<JsonValue> {
        self.skip_ws();
        let v = self.value()?;
        Ok(v)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() { self.pos += 1; }
    }

    fn value(&mut self) -> Result<JsonValue> {
        self.skip_ws();
        if self.pos >= self.chars.len() {
            return Err(self.err("意外的 JSON 结束"));
        }
        match self.chars[self.pos] {
            '{' => self.object(),
            '[' => self.array(),
            '"' => Ok(JsonValue::String(self.string()?)),
            't' | 'f' => self.boolean(),
            'n' => self.null(),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Result<JsonValue> {
        self.pos += 1; // {
        let mut map = HashMap::new();
        self.skip_ws();
        if self.pos < self.chars.len() && self.chars[self.pos] == '}' { self.pos += 1; return Ok(JsonValue::Object(map)); }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            if self.pos >= self.chars.len() || self.chars[self.pos] != ':' { return Err(self.err("期望 ':'")); }
            self.pos += 1;
            let val = self.value()?;
            map.insert(key, val);
            self.skip_ws();
            if self.pos >= self.chars.len() { return Err(self.err("对象未闭合")); }
            match self.chars[self.pos] {
                ',' => { self.pos += 1; }
                '}' => { self.pos += 1; break; }
                _ => return Err(self.err("期望 ',' 或 '}'")),
            }
        }
        Ok(JsonValue::Object(map))
    }

    fn array(&mut self) -> Result<JsonValue> {
        self.pos += 1; // [
        let mut arr = Vec::new();
        self.skip_ws();
        if self.pos < self.chars.len() && self.chars[self.pos] == ']' { self.pos += 1; return Ok(JsonValue::Array(arr)); }
        loop {
            let val = self.value()?;
            arr.push(val);
            self.skip_ws();
            if self.pos >= self.chars.len() { return Err(self.err("数组未闭合")); }
            match self.chars[self.pos] {
                ',' => { self.pos += 1; }
                ']' => { self.pos += 1; break; }
                _ => return Err(self.err("期望 ',' 或 ']'")),
            }
        }
        Ok(JsonValue::Array(arr))
    }

    fn string(&mut self) -> Result<String> {
        if self.chars[self.pos] != '"' { return Err(self.err("期望 '\"'")); }
        self.pos += 1;
        let mut s = String::new();
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c == '"' { self.pos += 1; return Ok(s); }
            if c == '\\' {
                self.pos += 1;
                if self.pos >= self.chars.len() { return Err(self.err("字符串转义未完成")); }
                let esc = self.chars[self.pos];
                let ch = match esc {
                    '"' => '"', '\\' => '\\', '/' => '/', 'n' => '\n', 't' => '\t',
                    'r' => '\r', 'b' => '\u{0008}', 'f' => '\u{000C}',
                    'u' => {
                        if self.pos + 4 >= self.chars.len() { return Err(self.err("\\u 转义不完整")); }
                        let hex: String = self.chars[self.pos + 1..self.pos + 5].iter().collect();
                        let code = u32::from_str_radix(&hex, 16).map_err(|_| self.err("\\u 十六进制非法"))?;
                        self.pos += 4;
                        char::from_u32(code).unwrap_or('\u{FFFD}')
                    }
                    _ => return Err(self.err("未知转义")),
                };
                s.push(ch);
                self.pos += 1;
            } else {
                s.push(c);
                self.pos += 1;
            }
        }
        Err(self.err("字符串未闭合"))
    }

    fn number(&mut self) -> Result<JsonValue> {
        let start = self.pos;
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E' {
                self.pos += 1;
            } else { break; }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        let n: f64 = s.parse().map_err(|_| self.err(format!("数字非法: {s}")))?;
        Ok(JsonValue::Number(n))
    }

    fn boolean(&mut self) -> Result<JsonValue> {
        if self.match_lit("true") { return Ok(JsonValue::Bool(true)); }
        if self.match_lit("false") { return Ok(JsonValue::Bool(false)); }
        Err(self.err("非法字面量"))
    }
    fn null(&mut self) -> Result<JsonValue> {
        if self.match_lit("null") { return Ok(JsonValue::Null); }
        Err(self.err("非法字面量"))
    }
    fn match_lit(&mut self, lit: &str) -> bool {
        let l: Vec<char> = lit.chars().collect();
        if self.pos + l.len() <= self.chars.len() && self.chars[self.pos..self.pos + l.len()] == l[..] {
            self.pos += l.len();
            true
        } else { false }
    }

    fn err(&self, msg: impl Into<String>) -> crate::error::MarkupError {
        crate::error::MarkupError::Render { message: format!("JSON 解析失败（pos {}）：{}", self.pos, msg.into()) }
    }
}

/// 把文献库写出为规范兼容的 JSON 数组（用于回写 `references-index.json` 摘要）。
pub fn references_to_json(refs: &dyn ReferenceProvider, ids: &[String]) -> Result<String> {
    let mut out = String::from("[\n");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 { out.push_str(",\n"); }
        let entry = refs.get(id);
        out.push_str("  {");
        out.push_str(&format!(" \"id\": \"{}\"", escape_json_string(id)));
        if let Some(e) = entry {
            if !e.authors.is_empty() {
                out.push_str(", \"authors\": [");
                for (j, a) in e.authors.iter().enumerate() {
                    if j > 0 { out.push_str(", "); }
                    out.push_str(&format!("\"{}\"", escape_json_string(a)));
                }
                out.push(']');
            }
            if let Some(y) = &e.year {
                out.push_str(&format!(", \"year\": \"{}\"", escape_json_string(y)));
            }
            if let Some(t) = &e.title {
                out.push_str(&format!(", \"title\": \"{}\"", escape_json_string(t)));
            }
        }
        out.push_str(" }");
    }
    out.push_str("\n]\n");
    Ok(out)
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
