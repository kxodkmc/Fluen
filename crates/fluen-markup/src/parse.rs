//! f- 标签扫描与属性解析，以及整体解析编排。
//!
//! 解析流程（规范 §8）：
//!   1. 抽取 YAML front matter（宽松：仅识别 `---`/`...` 边界，键值原样保留）。
//!   2. 逐行扫描 `<f-...>` 标签（自闭合 / 配对），按规范 §5「块级标签独占一行」。
//!   3. f- 标签之间的纯 Markdown 文本累积后一次性交给 [`md_block`] 解析。
//!   4. f- 标签内部内容按规范二次解析：
//!        - `<f-eq>`：原始 LaTeX 文本（不解析）；
//!        - `<f-fig>`/`<f-tbl>` 内 `<f-caption>`：行内二次解析；
//!        - `<f-claim>`：块级二次解析；
//!        - `<f-tbl>` 形态 B：Markdown 表；形态 C：HTML `<table>` 子集；
//!        - `<f-xref>` 手写覆盖内容：行内解析。
//!   5. `<f-cite>`/`<f-xref>` 行内形态：在 `md_inline` 之前先从段落文本中抽取，
//!      用占位符替换，行内解析后再回填——本实现采用"先扫描整行 inline 标签再解析"
//!      的两段式，见 [`parse_inline_with_f_tags`]。
//!
//! 关键设计：f- 标签块与 Markdown 块的"交错"通过"分段累积"实现，避免与
//! Markdown 段落拼接歧义（规范 §5：块级标签前后须空行）。

use crate::error::{MarkupError, Result};
use crate::html_table::{self, AttrMap};
use crate::inline_tags;
use crate::kinds::{ClaimType, IdKind, TableFormat, TableSource, TableVariant};
use crate::md_block::parse_blocks;
use crate::md_inline::parse_inline;
use crate::model::*;

/// 解析单份 Markdown 文本（一个 `sec-*.md`）为 [`Document`]（含单个 Section）。
/// `base_line` 通常为 1；多章节组装由宿主拼接多个 Section 后再调用 [`crate::resolve`]。
pub fn parse_document(text: &str) -> Result<Document> {
    let (front, body, _body_offset_line) = split_front_matter(text);
    let section = parse_section(&body, 1, front.title())?;
    Ok(Document { front_matter: front, sections: vec![section] })
}

/// 解析为 Section（给定起始行号与可选标题）。
pub fn parse_section(text: &str, base_line: usize, fallback_title: Option<&str>) -> Result<Section> {
    let mut scanner = TagScanner::new(text, base_line);
    let blocks = scanner.scan_to_blocks()?;
    // 标题优先取首个 H1，否则 fallback
    let title = first_h1_text(&blocks).or_else(|| fallback_title.map(str::to_string)).unwrap_or_default();
    let id = first_h1_id(&blocks);
    Ok(Section { title, id, blocks, line: base_line })
}

// ───────────────────────── Front matter ─────────────────────────

/// 抽取 YAML front matter。返回 (front, body, body 起始行号)。
/// 宽松解析：仅识别首行 `---` 与其后的 `---`/`...` 边界；键值按 `key: value` 收集。
fn split_front_matter(text: &str) -> (FrontMatter, String, usize) {
    let mut lines = text.lines();
    let first = lines.next();
    let mut entries = Vec::new();
    let mut consumed = 1usize;
    let mut title = None::<String>;
    if first.map(|l| l.trim() == "---").unwrap_or(false) {
        // 收集直到结束边界
        let mut body_lines: Vec<&str> = Vec::new();
        let mut in_fm = true;
        for l in lines {
            consumed += 1;
            let t = l.trim();
            if in_fm {
                if t == "---" || t == "..." {
                    in_fm = false;
                    continue;
                }
                // key: value（简化）
                if let Some(idx) = l.find(':') {
                    let key = l[..idx].trim().to_string();
                    let val = l[idx + 1..].trim().trim_matches('"').trim_matches('\'').to_string();
                    if key.eq_ignore_ascii_case("title") {
                        title = Some(val.clone());
                    }
                    entries.push((key, val));
                }
            } else {
                body_lines.push(l);
            }
        }
        let mut fm = FrontMatter { entries };
        if let Some(t) = title { fm.entries.insert(0, ("title".to_string(), t)); }
        (fm, body_lines.join("\n"), consumed + 1)
    } else {
        (FrontMatter::default(), text.to_string(), 1)
    }
}

fn first_h1_text(blocks: &[Block]) -> Option<String> {
    for b in blocks {
        if let Block::Heading { level: 1, text, .. } = b {
            return Some(inlines_to_plain(text));
        }
    }
    None
}
fn first_h1_id(blocks: &[Block]) -> Option<String> {
    for b in blocks {
        if let Block::Heading { level: 1, id, .. } = b {
            if id.is_some() { return id.clone(); }
        }
    }
    None
}

/// 行内序列转为纯文本（用于标题字面值）。
pub(crate) fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for i in inlines {
        push_plain(i, &mut s);
    }
    s
}
fn push_plain(i: &Inline, out: &mut String) {
    match i {
        Inline::Text(t) => out.push_str(t),
        Inline::Code(c) => out.push_str(c),
        Inline::Math(m) => out.push_str(m),
        Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => {
            for x in v { push_plain(x, out); }
        }
        Inline::Link { text, .. } => for x in text { push_plain(x, out); },
        Inline::SoftBreak => out.push(' '),
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::Cite(c) => {
            if let Some(fb) = &c.fallback { out.push_str(fb); }
            else { out.push_str(&c.refs.join(", ")); }
        }
        Inline::Xref(x) => {
            if let Some(fb) = &x.fallback { out.push_str(fb); }
            else { out.push_str(&x.to); }
        }
    }
}

// ═══════════════════════════ 标签扫描 ═══════════════════════════

#[allow(dead_code)]
struct TagScanner<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    base_line: usize,
    pos: usize,
    /// 累积的纯 Markdown 文本（标签之间）。
    md_buf: String,
    /// md_buf 对应的起始行号。
    md_buf_line: usize,
}

impl<'a> TagScanner<'a> {
    fn new(text: &'a str, base_line: usize) -> Self {
        Self {
            text,
            lines: text.lines().collect(),
            base_line,
            pos: 0,
            md_buf: String::new(),
            md_buf_line: base_line,
        }
    }

    /// 主扫描：把整段文本转为 Block 序列。先收集所有"段"（标签块或 Markdown 段），
    /// 再按出现顺序拼接。
    fn scan_to_blocks(&mut self) -> Result<Vec<Block>> {
        // 用有序列表保存"片段"：Markdown 文本段 与 f-标签块，保持原始顺序。
        let mut segments: Vec<Segment> = Vec::new();
        // 追踪当前 Markdown 缓冲区的起始行（首个非空）
        let mut md_start: Option<usize> = None;

        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            let line_no = self.base_line + self.pos;
            let trimmed = raw.trim_start();

            // 忽略 HTML 注释行（如章节边界标记 <!-- section-boundary -->）
            if trimmed.starts_with("<!--") {
                self.pos += 1;
                continue;
            }

            // 识别块级 f- 标签起始
            if let Some(tag_name) = block_tag_name(trimmed) {
                // 先 flush 累积的 Markdown
                if !self.md_buf.trim().is_empty() {
                    segments.push(Segment::Markdown(std::mem::take(&mut self.md_buf), md_start.unwrap_or(line_no)));
                    md_start = None;
                }
                let block = self.parse_block_tag(tag_name, trimmed, line_no)?;
                segments.push(Segment::Block(block));
                continue;
            }

            // 普通行：累积到 md_buf
            if md_start.is_none() && !trimmed.is_empty() {
                md_start = Some(line_no);
            }
            self.md_buf.push_str(raw);
            self.md_buf.push('\n');
            self.pos += 1;
        }
        // flush 末尾
        if !self.md_buf.trim().is_empty() {
            segments.push(Segment::Markdown(std::mem::take(&mut self.md_buf), md_start.unwrap_or(self.base_line)));
        }

        // 转换：Markdown 段经 md_block 解析；Block 段直接保留。
        let mut out = Vec::with_capacity(segments.len());
        for seg in segments {
            match seg {
                Segment::Block(b) => out.push(b),
                Segment::Markdown(text, line) => {
                    let mut blocks = parse_blocks(&text, line);
                    // 行内 f- 标签（cite/xref）后处理
                    inline_tags::post_process_inlines(&mut blocks);
                    out.extend(blocks);
                }
            }
        }
        Ok(out)
    }

    /// 解析一个块级 f- 标签。`first_line` 为去掉前导空白的该行。
    fn parse_block_tag(&mut self, name: &str, first_line: &str, line_no: usize) -> Result<Block> {
        match name {
            "f-eq" => self.parse_eq(first_line, line_no).map(Block::Equation),
            "f-fig" => self.parse_fig(first_line, line_no).map(Block::Figure),
            "f-tbl" => self.parse_tbl(first_line, line_no).map(Block::Table),
            "f-claim" => self.parse_claim(first_line, line_no).map(Block::Claim),
            // f-caption 不会出现在顶层（仅 fig/tbl 内），行内标签同理
            other => Err(MarkupError::parse(format!("未预期的块级标签 <{other}>"), line_no)),
        }
    }

    // ── <f-eq> ──
    fn parse_eq(&mut self, first_line: &str, line_no: usize) -> Result<FluenEq> {
        let (attrs, kind, body, consumed) = self.read_tag(first_line, line_no)?;
        let id = attrs.get("id").cloned();
        // eq 仅允许自闭合外的纯文本；不允许 inner 标签（规范 §5.1）
        if matches!(kind, TagKind::Paired) && body.contains('<') {
            return Err(MarkupError::parse("<f-eq> 内禁止任何标签（规范 §5.1）", line_no));
        }
        self.pos += consumed;
        Ok(FluenEq { id, latex: body.trim().to_string(), line: line_no, resolved_number: None })
    }

    // ── <f-fig> ──
    fn parse_fig(&mut self, first_line: &str, line_no: usize) -> Result<FluenFig> {
        let (attrs, _kind, body, consumed) = self.read_tag(first_line, line_no)?;
        let id = attrs.get("id").cloned();
        let src = attrs.get("src").cloned().ok_or_else(|| MarkupError::parse("<f-fig> 缺少必填属性 src", line_no))?;
        let alt = attrs.get("alt").cloned();
        // 解析 caption：内部有且仅有一个 <f-caption>
        let caption = extract_caption(&body, line_no)?;
        self.pos += consumed;
        Ok(FluenFig { id, src, alt, caption, line: line_no, resolved_number: None })
    }

    // ── <f-tbl> ──
    fn parse_tbl(&mut self, first_line: &str, line_no: usize) -> Result<FluenTbl> {
        let (attrs, _kind, body, consumed) = self.read_tag(first_line, line_no)?;
        let id = attrs.get("id").cloned();
        // variant
        let variant = match attrs.get("variant") {
            Some(v) => TableVariant::from_attr(v).ok_or_else(|| MarkupError::parse(format!("<f-tbl variant=\"{v}\"> 取值非法（仅 threeline/grid）"), line_no))?,
            None => TableVariant::default(),
        };
        // 表体来源：src 属性 ｜ 内嵌 MD 表 ｜ 内嵌 HTML <table>（互斥）
        let src_attr = attrs.get("src").cloned();
        let caption = extract_caption(&body, line_no)?;
        // 去掉 caption 后的表体文本
        let table_body = remove_caption_block(&body);
        let source = if let Some(path) = src_attr {
            // 形态 A：外部数据
            if html_table::has_inner_table(&table_body) {
                return Err(MarkupError::parse("<f-tbl> 形态互斥：不可同时有 src 与内嵌表", line_no));
            }
            let ext = path.rsplit('.').next().unwrap_or("");
            let format = TableFormat::from_ext(ext)
                .ok_or_else(|| MarkupError::parse(format!("<f-tbl src> 不支持的文件扩展名 `.{ext}`（仅 csv/json）"), line_no))?;
            TableSource::External { path, format }
        } else if let Some(tbl) = html_table::find_html_table(&table_body) {
            // 形态 C：内嵌 HTML <table>
            if html_table::find_html_table_2nd(&table_body).is_some() {
                return Err(MarkupError::parse("<f-tbl> 仅允许一个内嵌 <table>", line_no));
            }
            TableSource::Html(html_table::parse_html_table_subset(tbl, line_no)?)
        } else {
            // 形态 B：内嵌 Markdown 表
            match crate::md_block::parse_markdown_table(&table_body, line_no) {
                Some(model) => TableSource::Markdown(model),
                None => return Err(MarkupError::parse("<f-tbl> 未找到有效表体（src / MD 表 / HTML 表 三者居其一）", line_no)),
            }
        };
        self.pos += consumed;
        Ok(FluenTbl { id, source, variant, caption, line: line_no, resolved_number: None })
    }

    // ── <f-claim> ──
    fn parse_claim(&mut self, first_line: &str, line_no: usize) -> Result<FluenClaim> {
        let (attrs, _kind, body, consumed) = self.read_tag(first_line, line_no)?;
        let id = attrs.get("id").cloned();
        let ty_raw = attrs.get("type").cloned().ok_or_else(|| MarkupError::parse("<f-claim> 缺少必填属性 type", line_no))?;
        let ty = ClaimType::from_attr(&ty_raw)
            .ok_or_else(|| MarkupError::parse(format!("<f-claim type=\"{ty_raw}\"> 取值非法（见规范 §3.1）"), line_no))?;
        // 校验 type ↔ id 前缀（规范 §3.1）
        if let Some(idv) = &id {
            if !idv.starts_with(ty.to_id_prefix()) {
                return Err(MarkupError::lint(crate::error::claim_prefix_mismatch(&ty, idv), line_no, crate::error::Severity::Error));
            }
        }
        // 正文块级二次解析
        let body_blocks = parse_blocks(&body, line_no);
        let mut body_blocks = body_blocks;
        inline_tags::post_process_inlines(&mut body_blocks);
        self.pos += consumed;
        Ok(FluenClaim { id, ty, body: body_blocks, line: line_no, resolved_number: None })
    }

    // ── 通用：读取一个标签的开头，决定自闭合 / 配对，返回 (属性, body, 跨越行数) ──
    fn read_tag(&mut self, first_line: &str, line_no: usize) -> Result<(AttrMap, TagKind, String, usize)> {
        // 解析起始行：取标签名 + 属性 + 是否自闭合 `/ >`
        let opening = first_line;
        let (tag, attrs, self_closing, after_open) = parse_opening_tag(opening, line_no)?;
        if self_closing {
            return Ok((attrs, TagKind::SelfClosing, String::new(), 1));
        }
        // 配对：先检查起始行自身是否包含闭合标签（单行形态）
        let close = format!("</{}>", tag);
        if let Some(end) = after_open.find(&close) {
            let body = after_open[..end].to_string();
            return Ok((attrs, TagKind::Paired, body, 1));
        }
        // 跨行：寻找 `</tag>`，内容为中间所有行
        let mut body_lines: Vec<&str> = Vec::new();
        let mut i = self.pos + 1;
        let close_trim_start = close.clone();
        while i < self.lines.len() {
            let t = self.lines[i].trim();
            // 允许闭合标签后随空白
            if t == close_trim_start || t.starts_with(&close) {
                // 收集 body（注意：body 不含起始/闭合行）
                let body = body_lines.join("\n");
                return Ok((attrs, TagKind::Paired, body, (i - self.pos) + 1));
            }
            body_lines.push(self.lines[i]);
            i += 1;
        }
        Err(MarkupError::parse(format!("未找到闭合标签 </{tag}>"), line_no))
    }
}

#[derive(Debug)]
enum Segment { Markdown(String, usize), Block(Block) }

#[derive(Debug)]
enum TagKind { SelfClosing, Paired }

/// 判断某行是否以块级 f- 标签开头，返回标签名（去 `f-` 前缀的纯名，如 "f-eq"）。
fn block_tag_name(line: &str) -> Option<&'static str> {
    let t = line.trim_start();
    for name in ["f-eq", "f-fig", "f-tbl", "f-claim"] {
        if t.starts_with(&format!("<{name}")) {
            let after = &t[name.len() + 1..];
            // 必须后接空格 / `>` / `/`，避免 <f-figx> 误匹配
            if after.is_empty() || matches!(after.as_bytes()[0], b' ' | b'>' | b'/') {
                return Some(name);
            }
        }
    }
    None
}

/// 解析起始标签：返回 (标签名, 属性, 是否自闭合, 起始标签之后的剩余文本)。
/// 形如 `<f-fig id="x" src='y.png'>` 或 `<f-cite ref="a"/>`。
fn parse_opening_tag(line: &str, line_no: usize) -> Result<(String, AttrMap, bool, String)> {
    let t = line.trim();
    let after_lt = t.strip_prefix('<').ok_or_else(|| MarkupError::parse("标签缺少 <", line_no))?;
    // 标签名：字母/数字/-
    let mut chars = after_lt.char_indices();
    let name_end = chars
        .by_ref()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '-')
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if name_end == 0 {
        return Err(MarkupError::parse("标签名解析失败", line_no));
    }
    let tag = &after_lt[..name_end];
    let mut rest = after_lt[name_end..].trim_start();

    let mut attrs: AttrMap = AttrMap::new();
    let mut self_closing = false;
    let after_tag;
    // 逐属性解析直到 `>` 或 `/ >`
    loop {
        let r = rest.trim_start();
        if r.is_empty() {
            return Err(MarkupError::parse(format!("标签 <{tag}> 未在行内闭合"), line_no));
        }
        if let Some(tail) = r.strip_prefix("/>") {
            self_closing = true;
            after_tag = tail.to_string();
            break;
        }
        if let Some(tail) = r.strip_prefix('>') {
            after_tag = tail.to_string();
            break;
        }
        // 属性名
        let an_end = r
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ':')
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if an_end == 0 {
            return Err(MarkupError::parse(format!("标签 <{tag}> 属性解析失败：`{r}`"), line_no));
        }
        let aname = r[..an_end].to_string();
        let mut r2 = r[an_end..].trim_start();
        // 无值属性（如 `disabled`）——本规范暂无此类，按空串处理
        if !r2.starts_with('=') {
            attrs.insert(aname, String::new());
            rest = r2;
            continue;
        }
        r2 = r2[1..].trim_start();
        // 引号
        let (val, consumed) = if let Some(q) = r2.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let after_q = &r2[q.len_utf8()..];
            let end = after_q.find(q).ok_or_else(|| MarkupError::parse(format!("标签 <{tag}> 属性 {aname} 引号未闭合"), line_no))?;
            (after_q[..end].to_string(), end + 2 * q.len_utf8())
        } else {
            // 无引号值：到空白或 `>` 或 `/>`
            let end = r2.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(r2.len());
            (r2[..end].to_string(), end)
        };
        attrs.insert(aname, val);
        rest = &r2[consumed..];
    }
    Ok((tag.to_string(), attrs, self_closing, after_tag))
}

// ═══════════════════════════ caption 抽取 ═══════════════════════════

/// 从 fig/tbl 的 body 中抽取 `<f-caption>` 的行内内容。要求恰有一个。
fn extract_caption(body: &str, line_no: usize) -> Result<Vec<Inline>> {
    let caps = find_all_captions(body);
    match caps.len() {
        0 => Err(MarkupError::lint("<f-fig>/<f-tbl> 内缺少 <f-caption>（规范 §6.2 要求恰为 1）", line_no, crate::error::Severity::Error)),
        1 => {
            let cap = &caps[0];
            let inlines = parse_inline(cap.trim());
            Ok(inlines)
        }
        _ => Err(MarkupError::lint("<f-fig>/<f-tbl> 内 <f-caption> 多于 1 个（规范 §6.2 要求恰为 1）", line_no, crate::error::Severity::Error)),
    }
}

/// body 中所有 `<f-caption>...</f-caption>` 的文本内容。
fn find_all_captions(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut s = body;
    while let Some(start) = s.find("<f-caption") {
        let after_open = &s[start..];
        // 跳过属性到 `>`
        let gt = match after_open.find('>') {
            Some(g) => g,
            None => break,
        };
        let rest = &after_open[gt + 1..];
        if let Some(end) = rest.find("</f-caption>") {
            out.push(rest[..end].to_string());
            s = &rest[end + "</f-caption>".len()..];
        } else {
            break;
        }
    }
    out
}

/// 从 fig/tbl body 中移除 `<f-caption>...</f-caption>` 块（用于 tbl 表体提取）。
fn remove_caption_block(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut s = body;
    let mut last = 0usize;
    while let Some(start) = s.find("<f-caption") {
        out.push_str(&s[..start]);
        let after_open = &s[start..];
        let gt = match after_open.find('>') { Some(g) => g, None => break };
        let rest = &after_open[gt + 1..];
        match rest.find("</f-caption>") {
            Some(end) => {
                last = 0;
                s = &rest[end + "</f-caption>".len()..];
            }
            None => break,
        }
    }
    out.push_str(s);
    let _ = last;
    out
}

// ═══════════════════════════ 公开便利 ═══════════════════════════

// ── 公开便利：从 Section 收集所有 id（供 resolve 复用）──
#[allow(dead_code)]
pub(crate) fn collect_ids(sections: &[Section]) -> Vec<(String, IdKind, usize)> {
    let mut v = Vec::new();
    for s in sections {
        collect_ids_blocks(&s.blocks, &mut v);
        if let Some(id) = &s.id {
            if let Some(kind) = IdKind::from_id(id) {
                v.push((id.clone(), kind, s.line));
            }
        }
    }
    v
}
#[allow(dead_code)]
fn collect_ids_blocks(blocks: &[Block], out: &mut Vec<(String, IdKind, usize)>) {
    for b in blocks {
        match b {
            Block::Equation(e) => {
                if let Some(id) = &e.id { out.push((id.clone(), IdKind::Equation, e.line)); }
            }
            Block::Figure(f) => {
                if let Some(id) = &f.id { out.push((id.clone(), IdKind::Figure, f.line)); }
            }
            Block::Table(t) => {
                if let Some(id) = &t.id { out.push((id.clone(), IdKind::Table, t.line)); }
            }
            Block::Claim(c) => {
                if let Some(id) = &c.id { out.push((id.clone(), IdKind::Claim(c.ty), c.line)); }
            }
            Block::Heading { id: Some(id), .. } => {
                if let Some(kind) = IdKind::from_id(id) { out.push((id.clone(), kind, 0)); }
            }
            Block::BlockQuote(inner, _) => collect_ids_blocks(inner, out),
            _ => {}
        }
    }
}
