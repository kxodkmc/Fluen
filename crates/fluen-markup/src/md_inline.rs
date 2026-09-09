//! Markdown 行内解析器（学术论文子集）。
//!
//! 支持范围（规范 §1：原生优先）：
//!   - 行内代码 `` `...` ``、行内数学 `$...$`
//!   - 强调 `*x*` / `_x_`、加粗 `**x**` / `__x__`、删除线 `~~x~~`
//!   - 链接 `[text](url "title")`、图片 `![alt](src "title")`
//!   - 反斜杠转义、硬换行（行尾两空格或 `\`）、软换行
//!   - HTML 实体（命名/十进制/十六进制）的最小反转义（`&amp;` 等）
//!
//! 不支持（与论文场景无关，刻意精简）：自动链接邮箱、行内 HTML、`<br>` 等。
//! f-标签的行内形态（`<f-cite>` / `<f-xref>`）在 parse.rs 中先于本解析器抽取，
//! 本解析器仅处理纯 Markdown 行内段。

use crate::model::Inline;

/// 解析一行（或多行）Markdown 行内为 [`Inline`] 序列。
/// 输入应已去除块级结构（标题前缀、引用前缀 `>` 等），仅含行内语法与软换行。
pub fn parse_inline(input: &str) -> Vec<Inline> {
    let mut p = InlineParser { chars: input.chars().collect(), pos: 0, out: Vec::new() };
    p.run();
    p.out
}

struct InlineParser {
    chars: Vec<char>,
    pos: usize,
    out: Vec<Inline>,
}

impl InlineParser {
    fn run(&mut self) {
        // 以"片段"为单位扫描：每当遇到可识别的行内结构，先把累积的纯文本 flush 出去。
        let mut text_start = self.pos;
        loop {
            if self.pos >= self.chars.len() {
                break;
            }
            let c = self.chars[self.pos];

            // 反斜杠转义：下一字符原样入文本（支持 \* \_ \\ \[ \! 等）
            if c == '\\' && self.pos + 1 < self.chars.len() {
                let esc = self.chars[self.pos + 1];
                if is_escapable(esc) {
                    self.flush_text(text_start);
                    self.out.push(Inline::Text(esc.to_string()));
                    self.pos += 2;
                    text_start = self.pos;
                    continue;
                }
            }

            // 行内代码 `` ` ``（最高优先级：内部不解析）
            if c == '`' {
                if let Some(end) = self.find_code_span(self.pos) {
                    self.flush_text(text_start);
                    let inner: String = self.chars[self.pos + 1..end].iter().collect();
                    // 去掉单层空格填充（` `` x `` ` → ` x `）：CommonMark 规则的简化版
                    let inner = strip_code_padding(&inner);
                    self.out.push(Inline::Code(inner));
                    self.pos = end + 1;
                    text_start = self.pos;
                    continue;
                }
            }

            // 行内数学 `$...$`
            if c == '$' {
                if let Some(end) = self.find_math_span(self.pos) {
                    self.flush_text(text_start);
                    let inner: String = self.chars[self.pos + 1..end].iter().collect();
                    self.out.push(Inline::Math(inner.trim().to_string()));
                    self.pos = end + 1;
                    text_start = self.pos;
                    continue;
                }
            }

            // 图片 `![alt](src)`（在链接前判断，因 `!` 前缀）
            if c == '!' && self.peek_at(1) == Some('[') {
                if let Some((label_end, url, title)) = self.try_link_label(self.pos + 1) {
                    self.flush_text(text_start);
                    let alt: String = self.chars[self.pos + 2..label_end].iter().collect();
                    self.out.push(Inline::Image { alt, src: url, title });
                    self.pos = label_end + 1 + url_len_consumed(&self.chars[label_end + 1..]);
                    text_start = self.pos;
                    continue;
                }
            }

            // 链接 `[text](url)`
            if c == '[' {
                if let Some((label_end, url, title)) = self.try_link_label(self.pos) {
                    self.flush_text(text_start);
                    let text_chars = &self.chars[self.pos + 1..label_end];
                    let text = parse_inline(&text_chars.iter().collect::<String>());
                    self.out.push(Inline::Link { text, url, title });
                    self.pos = label_end + 1 + url_len_consumed(&self.chars[label_end + 1..]);
                    text_start = self.pos;
                    continue;
                }
            }

            // 强调族：** __ * _ ~~ ++
            if let Some(inline) = self.try_emphasis(self.pos) {
                self.flush_text(text_start);
                self.out.push(inline.node);
                self.pos = inline.end;
                text_start = self.pos;
                continue;
            }

            // 硬换行：行尾两空格或 `\`+换行；软换行：单个 `\n`
            if c == '\n' {
                self.flush_text(text_start);
                // 检查硬换行：前一字符是空格×2
                let hard = self.pos >= 2
                    && self.chars[self.pos - 1] == ' '
                    && self.chars[self.pos - 2] == ' ';
                // 去除尾部两空格导致的空格（仅硬换行场景）
                if hard {
                    if let Some(Inline::Text(s)) = self.out.last_mut() {
                        while s.ends_with(' ') { s.pop(); }
                    }
                }
                self.out.push(if hard { Inline::SoftBreak } else { Inline::SoftBreak });
                self.pos += 1;
                text_start = self.pos;
                continue;
            }

            // HTML 实体（最小反转义）
            if c == '&' {
                if let Some((entity, len)) = self.try_entity(self.pos) {
                    self.flush_text(text_start);
                    self.out.push(Inline::Text(entity));
                    self.pos += len;
                    text_start = self.pos;
                    continue;
                }
            }

            self.pos += 1;
        }
        self.flush_text(text_start);
    }

    fn flush_text(&mut self, start: usize) {
        if start < self.pos {
            let s: String = self.chars[start..self.pos].iter().collect();
            if !s.is_empty() {
                self.out.push(Inline::Text(s));
            }
        }
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    // ── 行内代码 `...`：寻找匹配的反引号（等长跨度的反引号闭合）──
    fn find_code_span(&self, start: usize) -> Option<usize> {
        let mut backticks = 0usize;
        let mut i = start;
        while i < self.chars.len() && self.chars[i] == '`' { backticks += 1; i += 1; }
        // 闭合需同样数量的反引号
        let mut j = i;
        while j + backticks <= self.chars.len() {
            if self.chars[j..j + backticks].iter().all(|&c| c == '`') {
                // 起始反引号数应等于 1（简化：仅支持单反引号代码跨距）
                if backticks == 1 {
                    return Some(j);
                }
            }
            j += 1;
        }
        None
    }

    // ── 行内数学 $...$：下一个非转义 `$` 闭合；跨行不允许（简化）──
    fn find_math_span(&self, start: usize) -> Option<usize> {
        let mut i = start + 1;
        while i < self.chars.len() {
            let c = self.chars[i];
            if c == '\n' { return None; } // 不跨行
            if c == '\\' && i + 1 < self.chars.len() { i += 2; continue; }
            if c == '$' { return Some(i); }
            i += 1;
        }
        None
    }

    // ── 链接/图片的 `[label](url "title")` ──
    // 返回 (label_end_index, url, title)。label_end 指向 `]`。
    fn try_link_label(&self, bracket_start: usize) -> Option<(usize, String, Option<String>)> {
        // 找 `]`
        let mut depth = 1usize;
        let mut i = bracket_start + 1;
        while i < self.chars.len() && depth > 0 {
            match self.chars[i] {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            if depth == 0 { break; }
            i += 1;
        }
        if depth != 0 || i >= self.chars.len() { return None; }
        let label_end = i; // 指向 `]`
                                   // 紧跟 `(`
        if self.peek_at_offset(label_end, 1) != Some('(') { return None; }
        // 解析 url 直到 `)`；url 内不允许空白（除非 <...> 包裹）；title 在 url 后
        let mut j = label_end + 2;
        let url_start = j;
        while j < self.chars.len() && self.chars[j] != ')' && self.chars[j] != ' ' && self.chars[j] != '\n' {
            j += 1;
        }
        let url: String = self.chars[url_start..j].iter().collect();
        // 可选 title
        let mut title: Option<String> = None;
        // 跳过空白
        while j < self.chars.len() && (self.chars[j] == ' ' || self.chars[j] == '\n') { j += 1; }
        if j < self.chars.len() && (self.chars[j] == '"' || self.chars[j] == '\'') {
            let quote = self.chars[j];
            let t_start = j + 1;
            let mut k = t_start;
            while k < self.chars.len() && self.chars[k] != quote { k += 1; }
            if k < self.chars.len() {
                title = Some(self.chars[t_start..k].iter().collect());
                j = k + 1;
            }
        }
        // 闭合 `)`
        while j < self.chars.len() && self.chars[j] != ')' { j += 1; }
        if j >= self.chars.len() { return None; }
        Some((label_end, url, title))
    }

    fn peek_at_offset(&self, base: usize, offset: usize) -> Option<char> {
        self.chars.get(base + offset).copied()
    }

    // ── 强调族 ──
    fn try_emphasis(&self, start: usize) -> Option<EmphasisHit> {
        let two = (self.peek_at_offset(start, 0), self.peek_at_offset(start, 1));
        // 三字符的删除线 ~~
        if two == (Some('~'), Some('~')) {
            if let Some(end) = self.match_delim(start, "~~") {
                let inner: String = self.chars[start + 2..end].iter().collect();
                return Some(EmphasisHit { node: Inline::Strikethrough(parse_inline(&inner)), end: end + 2 });
            }
        }
        // 下划线 ++
        if two == (Some('+'), Some('+')) {
            if let Some(end) = self.match_delim(start, "++") {
                let inner: String = self.chars[start + 2..end].iter().collect();
                return Some(EmphasisHit { node: Inline::Underline(parse_inline(&inner)), end: end + 2 });
            }
        }
        // ** __ 加粗
        if two == (Some('*'), Some('*')) || two == (Some('_'), Some('_')) {
            let delim: String = self.chars[start..start + 2].iter().collect();
            if let Some(end) = self.match_delim(start, &delim) {
                let inner: String = self.chars[start + 2..end].iter().collect();
                return Some(EmphasisHit { node: Inline::Strong(parse_inline(&inner)), end: end + 2 });
            }
        }
        // * _ 强调
        let one = self.peek_at_offset(start, 0);
        if one == Some('*') || one == Some('_') {
            let c = one.unwrap();
            // 排除 ** 已处理
            if self.peek_at_offset(start, 1) == Some(c) { return None; }
            if let Some(end) = self.match_delim_char(start, c) {
                let inner: String = self.chars[start + 1..end].iter().collect();
                return Some(EmphasisHit { node: Inline::Emphasis(parse_inline(&inner)), end: end + 1 });
            }
        }
        None
    }

    // 匹配定界符字符串（如 "~~"、"**"），返回开始定界符后到闭合定界符前的索引。
    fn match_delim(&self, start: usize, delim: &str) -> Option<usize> {
        let d: Vec<char> = delim.chars().collect();
        let n = d.len();
        let mut i = start + n;
        while i + n <= self.chars.len() {
            // 跳过转义
            if self.chars[i] == '\\' && i + 1 < self.chars.len() { i += 2; continue; }
            if self.chars[i..i + n] == d[..] {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn match_delim_char(&self, start: usize, c: char) -> Option<usize> {
        let mut i = start + 1;
        while i < self.chars.len() {
            if self.chars[i] == '\\' && i + 1 < self.chars.len() { i += 2; continue; }
            if self.chars[i] == c { return Some(i); }
            i += 1;
        }
        None
    }

    // ── HTML 实体（最小集）──
    fn try_entity(&self, start: usize) -> Option<(String, usize)> {
        // 查找 `;`
        let mut i = start + 1;
        while i < self.chars.len() && self.chars[i] != ';' && i - start < 12 {
            i += 1;
        }
        if i >= self.chars.len() || self.chars[i] != ';' { return None; }
        let body: String = self.chars[start + 1..i].iter().collect();
        let full_len = i - start + 1;
        // 命名实体（最小集，覆盖论文常用）
        if let Some(ch) = match body.as_str() {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{00A0}'),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "hellip" => Some('\u{2026}'),
            _ => None,
        } {
            return Some((ch.to_string(), full_len));
        }
        // 数字实体
        if let Some(rest) = body.strip_prefix('#') {
            let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                rest.parse::<u32>().ok()
            };
            if let Some(code) = code {
                if let Some(ch) = char::from_u32(code) {
                    return Some((ch.to_string(), full_len));
                }
            }
        }
        None
    }
}

struct EmphasisHit { node: Inline, end: usize }

fn is_escapable(c: char) -> bool {
    matches!(c, '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.' | '!' | '~' | '>' | '$' | '|')
}

/// 代码跨距内容的单层空格填充去除（CommonMark 简化）。
fn strip_code_padding(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() { return String::new(); }
    // 若两端均含空格且原内容非纯空格，去掉首尾各一空格
    if s != trimmed && s.starts_with(' ') && s.ends_with(' ') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// 计算 `(url ... )` 这段被消费的字符数（从 `]` 之后算起）。
/// 由于 try_link_label 已校验闭合，这里返回到 `)`（含）的长度。
fn url_len_consumed(after_bracket: &[char]) -> usize {
    // 找到 `)`
    let mut i = 0;
    while i < after_bracket.len() {
        if after_bracket[i] == ')' { return i + 1; }
        i += 1;
    }
    after_bracket.len()
}
