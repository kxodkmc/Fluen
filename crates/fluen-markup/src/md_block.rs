//! Markdown 块级解析器（学术论文子集）。
//!
//! 输入：一段文本（已由 parse.rs 抽取 f-标签块后剩余的 Markdown 文本，或 f-标签内部内容）。
//! 输出：[`Block`] 序列。
//!
//! 支持的块级结构（规范 §1：原生优先）：
//!   - ATX 标题 `#`..`######`（含可选行尾 `{#sec:…}` 锚点，规范 §3.3）
//!   - 段落（由空行分隔）
//!   - 围栏代码块 ``` ``` ```
//!   - 缩进代码块（4 空格）
//!   - 引用块 `>`（含多行、嵌套；内部递归为块）
//!   - 无序/有序列表 `-`/`*`/`+` 与 `1.`（含任务列表勾选 `[ ]`/`[x]`）
//!   - 分隔线 `---` / `***` / `___`
//!   - GFM 风格表格（作为段落内的特殊行组，识别后产出普通段落——本 crate 表格由 `<f-tbl>` 承载，
//!     原生 MD 表仅在 `<f-tbl>` 形态 B 内由 [`parse_markdown_table`] 专门处理）
//!
//! 行内内容（段落、标题、列表项）由 [`md_inline`] 二次解析。

use crate::md_inline::parse_inline;
use crate::model::{Block, Inline, ListItem};

/// 解析一段 Markdown 文本为块序列。`base_line` 为该文本起始行号（1-based）。
pub fn parse_blocks(text: &str, base_line: usize) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut bp = BlockParser { lines, pos: 0, base_line, out: Vec::new() };
    bp.run();
    bp.out
}

struct BlockParser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    base_line: usize,
    out: Vec<Block>,
}

impl<'a> BlockParser<'a> {
    fn run(&mut self) {
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            // 空行
            if raw.trim().is_empty() {
                self.pos += 1;
                continue;
            }
            let line_no = self.base_line + self.pos;

            // 围栏代码块
            if let Some(block) = self.try_fenced_code(line_no) {
                self.out.push(block);
                continue;
            }
            // ATX 标题
            if let Some(block) = self.try_heading(line_no) {
                self.out.push(block);
                self.pos += 1;
                continue;
            }
            // 分隔线
            if is_thematic_break(raw) {
                self.out.push(Block::ThematicBreak(line_no));
                self.pos += 1;
                continue;
            }
            // 引用块
            if raw.trim_start().starts_with('>') {
                let block = self.parse_blockquote(line_no);
                self.out.push(block);
                continue;
            }
            // 列表
            if let Some(ordered) = list_marker(raw) {
                let block = self.parse_list(ordered, line_no);
                self.out.push(block);
                continue;
            }
            // 缩进代码块（4 空格）
            if raw.starts_with("    ") {
                let block = self.parse_indented_code(line_no);
                self.out.push(block);
                continue;
            }
            // 段落（兜底）
            let block = self.parse_paragraph(line_no);
            self.out.push(block);
        }
    }

    #[allow(dead_code)]
    fn current_line_no(&self) -> usize { self.base_line + self.pos }

    // ── 围栏代码块 ──
    fn try_fenced_code(&mut self, line_no: usize) -> Option<Block> {
        let raw = self.lines[self.pos];
        let trimmed = raw.trim_start();
        let fence = trimmed.chars().next()?;
        if fence != '`' && fence != '~' { return None; }
        let fence_len = trimmed.chars().take_while(|&c| c == fence).count();
        if fence_len < 3 { return None; }
        let info = trimmed[fence_len..].trim();
        // 收集直到等长/更长围栏
        let mut code = String::new();
        let mut i = self.pos + 1;
        while i < self.lines.len() {
            let l = self.lines[i];
            let lt = l.trim_start();
            if lt.chars().take_while(|&c| c == fence).count() >= fence_len && lt.chars().all(|c| c == fence || c.is_whitespace()) {
                break;
            }
            code.push_str(l);
            code.push('\n');
            i += 1;
        }
        let lang = if info.is_empty() { None } else { Some(info.to_string()) };
        self.pos = i + 1;
        Some(Block::CodeBlock { lang, code, line: line_no })
    }

    // ── ATX 标题（含可选行尾 {#sec:…} 锚点）──
    fn try_heading(&self, line_no: usize) -> Option<Block> {
        let raw = self.lines[self.pos];
        let trimmed = raw.trim_start();
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if hashes == 0 || hashes > 6 { return None; }
        let after = &trimmed[hashes..];
        // 必须后接空格或为空
        if !after.is_empty() && !after.starts_with(' ') { return None; }
        let mut text = after.trim().to_string();
        // 闭合 `#` 序列（ATX 可选）
        if let Some(stripped) = strip_closing_hashes(&text) {
            text = stripped;
        }
        // 抽取行尾 {#sec:…} 锚点（规范 §3.3：Pandoc 风格）
        let mut id = None;
        if let Some(idx) = text.rfind("{#") {
            let tail = &text[idx..];
            if tail.ends_with('}') {
                let inner = &tail[2..tail.len() - 1];
                if !inner.chars().any(|c| c.is_whitespace()) {
                    id = Some(inner.to_string());
                    text = text[..idx].trim_end().to_string();
                }
            }
        }
        let inlines = parse_inline(&text);
        Some(Block::Heading { level: hashes as u8, text: inlines, id, line: line_no })
    }

    // ── 引用块 ──
    fn parse_blockquote(&mut self, line_no: usize) -> Block {
        let mut inner = String::new();
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            let t = raw.trim_start();
            if let Some(rest) = t.strip_prefix('>') {
                // 去掉一个 `>` 与一个可选空格
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                inner.push_str(rest);
                inner.push('\n');
                self.pos += 1;
            } else if raw.trim().is_empty() {
                break;
            } else {
                // 惰性续行（简化：仅当当前行非其他块起始）
                if is_block_boundary(raw) { break; }
                inner.push_str(raw);
                inner.push('\n');
                self.pos += 1;
            }
        }
        let inner_blocks = parse_blocks(&inner, line_no);
        Block::BlockQuote(inner_blocks, line_no)
    }

    // ── 列表 ──
    fn parse_list(&mut self, ordered: bool, line_no: usize) -> Block {
        let mut items: Vec<ListItem> = Vec::new();
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            if raw.trim().is_empty() {
                // 列表结束于空行（简化：不支持紧凑/松散切换的复杂规则）
                break;
            }
            if let Some((content, _consumed)) = strip_list_item(raw) {
                let inlines = parse_inline(&content);
                items.push(ListItem { content: inlines });
                self.pos += 1;
            } else if !raw.starts_with(' ') {
                // 非列表项且非缩进续行 → 列表结束
                break;
            } else {
                // 缩进续行并入当前项
                if let Some(last) = items.last_mut() {
                    let cont = raw.trim_start();
                    if !cont.is_empty() {
                        last.content.push(Inline::SoftBreak);
                        last.content.extend(parse_inline(cont));
                    }
                }
                self.pos += 1;
            }
        }
        Block::List { ordered, items, line: line_no }
    }

    // ── 缩进代码块 ──
    fn parse_indented_code(&mut self, line_no: usize) -> Block {
        let mut code = String::new();
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            if raw.starts_with("    ") {
                code.push_str(&raw[4..]);
                code.push('\n');
                self.pos += 1;
            } else if raw.trim().is_empty() {
                // 允许空行后续仍属代码块（简化：保留空行，结束于非缩进非空行）
                code.push('\n');
                self.pos += 1;
                // 探测下一非空行
                let mut k = self.pos;
                while k < self.lines.len() && self.lines[k].trim().is_empty() { k += 1; }
                if k < self.lines.len() && !self.lines[k].starts_with("    ") {
                    break;
                }
            } else {
                break;
            }
        }
        Block::CodeBlock { lang: None, code, line: line_no }
    }

    // ── 段落（兜底）──
    fn parse_paragraph(&mut self, line_no: usize) -> Block {
        let mut acc = String::new();
        let mut started = false;
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            if raw.trim().is_empty() { break; }
            if started && is_block_boundary(raw) { break; }
            acc.push_str(raw);
            acc.push('\n');
            started = true;
            self.pos += 1;
        }
        Block::Paragraph(parse_inline(acc.trim_end()), line_no)
    }
}

/// 段落何时被下一行打断（用于段落与列表的边界判定）。
fn is_block_boundary(raw: &str) -> bool {
    let t = raw.trim_start();
    t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with("```")
        || t.starts_with("~~~")
        || is_thematic_break(raw)
        || list_marker(raw).is_some()
}

/// 是否为分隔线 `---` / `***` / `___`（至少 3 个同字符，仅含空白）。
fn is_thematic_break(raw: &str) -> bool {
    let t: String = raw.chars().filter(|&c| !c.is_whitespace()).collect();
    if t.len() < 3 { return false; }
    let c = t.chars().next().unwrap();
    (c == '-' || c == '*' || c == '_') && t.chars().all(|x| x == c)
}

/// 列表标记：返回 Some(ordered) 与否。ordered=true 表示有序 `1.`。
fn list_marker(raw: &str) -> Option<bool> {
    let t = raw.trim_start();
    let bytes = t.as_bytes();
    if bytes.is_empty() { return None; }
    // 无序
    if bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+' {
        if bytes.len() == 1 || bytes[1] == b' ' || bytes[1] == b'\t' {
            return Some(false);
        }
    }
    // 有序：数字 + '.' 或 ')'
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    if i > 0 && i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b')') {
        if i + 1 == bytes.len() || bytes[i + 1] == b' ' || bytes[i + 1] == b'\t' {
            return Some(true);
        }
    }
    None
}

/// 剥离列表项标记，返回 (内容, 标记长度)。
fn strip_list_item(raw: &str) -> Option<(String, usize)> {
    let t = raw.trim_start();
    let lead = raw.len() - t.len();
    let bytes = t.as_bytes();
    if bytes.is_empty() { return None; }
    let (marker_len, ordered) = if bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+' {
        (1, false)
    } else {
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
        if i > 0 && i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b')') {
            (i + 1, true)
        } else {
            return None;
        }
    };
    let _ = ordered;
    let after = &t[marker_len..];
    let content = after.trim_start();
    Some((content.to_string(), lead + marker_len + (after.len() - content.len())))
}

/// 去除 ATX 标题结尾的 `#` 序列（仅当前面有空格）。
fn strip_closing_hashes(s: &str) -> Option<String> {
    let trimmed_end = s.trim_end();
    if !trimmed_end.ends_with('#') { return None; }
    // 必须有前导空格分隔
    let hash_run = trimmed_end.bytes().rev().take_while(|&b| b == b'#').count();
    if trimmed_end.len() == hash_run { return None; } // 整行都是 #
    let split = trimmed_end.len() - hash_run;
    if split == 0 || trimmed_end.as_bytes()[split - 1] != b' ' { return None; }
    Some(trimmed_end[..split].trim_end().to_string())
}

// ══════════════════ GFM 表格解析（<f-tbl> 形态 B 用）══════════════════

/// 解析内嵌 Markdown 表为 [`crate::model::TableModel`]。
/// 输入为表格片段（首行表头 + 分隔行 + 数据行）。失败返回 None。
/// 规范 §5.3 形态 B：简单表，无合并。
pub fn parse_markdown_table(text: &str, base_line: usize) -> Option<crate::model::TableModel> {
    use crate::model::{Cell, TableModel};
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 { return None; }
    let header = split_table_row(lines[0])?;
    let _aligns = parse_alignment_row(lines[1])?;
    let mut body: Vec<Vec<Cell>> = Vec::new();
    for (i, l) in lines.iter().enumerate().skip(2) {
        let row = split_table_row(l).unwrap_or_default();
        let cells: Vec<Cell> = row
            .iter()
            .map(|c| Cell::new(parse_inline(c), base_line + i))
            .collect();
        body.push(cells);
    }
    let header_cells: Vec<Cell> = header
        .iter()
        .map(|c| Cell::new(parse_inline(c), base_line))
        .collect();
    Some(TableModel { header: header_cells, body })
}

fn split_table_row(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.starts_with('|') && !t.ends_with('|') {
        // 仍尝试按 `|` 切分
    }
    // 去掉首尾 `|`
    let inner = t.trim_start_matches('|').trim_end_matches('|');
    if inner.is_empty() { return Some(Vec::new()); }
    // 简化：不在转义 `\|` 之外做复杂处理
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&nc) = chars.peek() {
                if nc == '|' { cur.push('|'); chars.next(); continue; }
            }
            cur.push(c);
        } else if c == '|' {
            cells.push(cur.trim().to_string());
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    Some(cells)
}

fn parse_alignment_row(line: &str) -> Option<Vec<()>> {
    let cells = split_table_row(line)?;
    if cells.is_empty() { return None; }
    for c in &cells {
        let ok = c.chars().all(|x| x == '-' || x == ':' || x == ' ') && c.contains('-');
        if !ok { return None; }
    }
    Some(vec![(); cells.len()])
}
