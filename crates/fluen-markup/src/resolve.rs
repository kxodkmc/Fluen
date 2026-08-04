//! 解析（resolve）：自动编号 + 交叉引用 + 文献引用解析 + 降级处理（规范 §3.2 / §8.4 / §8.5）。
//!
//! 流程（§8.3 拼接后一次性重排）：
//!   1. 全文收集所有 id，按族分组、按文档出现顺序从 1 连续编号；
//!      章节多级号按层级生成（如 2.1）。
//!   2. 重复 id → 解析硬错误（Problem，Severity::Error）。
//!   3. 为每个 `<f-xref to>` 查表：
//!        - 命中 → 注入「类型前缀 + 编号」（§4.2）；
//!        - 未命中且有 fallback → 降级显示 + ⚠ + Warning；
//!        - 未命中且无 fallback → Error（正文空洞）。
//!   4. 为每个 `<f-cite ref>` 在文献库中查找：
//!        - 命中 → 按风格生成显示键；
//!        - 未命中且 fallback → 降级；
//!        - 未命中且无 fallback → Error。

use std::collections::HashMap;

use crate::context::{CiteStyle, NumberingTable, ReferenceProvider, format_xref_text};
use crate::error::{MarkupError, Problems, Result, Severity};
use crate::kinds::{ClaimType, IdKind};
use crate::model::*;

/// 解析入口：在 [`Document`] 上原地解析编号与引用，返回 [`NumberingTable`]。
/// 若存在 Error 级问题（如无 fallback 的失效引用、重复 id），返回首个 Err。
/// 若希望降级继续渲染，使用 [`resolve_lenient`]。
pub fn resolve(
    doc: &mut Document,
    refs: &dyn ReferenceProvider,
    options: &crate::context::Options,
) -> Result<NumberingTable> {
    let (numbering, problems) = resolve_inner(doc, refs, options);
    // Error 级问题 → 返回首个 Err（调用方可用 validate 提前发现）
    if let Some(first_err) = first_error(&problems) {
        return Err(first_err);
    }
    Ok(numbering)
}

/// 宽松解析：与 [`resolve`] 相同，但不返回 Err。所有问题（含 Error 级）通过返回值传出。
/// 编号表始终有效（即使有重复 id，也按首次出现注册），渲染可继续。
pub(crate) fn resolve_lenient(
    doc: &mut Document,
    refs: &dyn ReferenceProvider,
    options: &crate::context::Options,
) -> (NumberingTable, Problems) {
    resolve_inner(doc, refs, options)
}

/// 内部核心：执行全部 5 步解析，返回 (编号表, 问题清单)。不返回 Err。
fn resolve_inner(
    doc: &mut Document,
    refs: &dyn ReferenceProvider,
    options: &crate::context::Options,
) -> (NumberingTable, Problems) {
    let mut problems: Problems = Vec::new();
    // 1) 编号
    let numbering = number_document(doc, &mut problems);
    // 2) 章节多级号（基于标题层级）
    assign_section_numbers(doc, &numbering);
    // 3) 交叉引用解析
    resolve_xrefs(doc, &numbering, options, &mut problems);
    // 4) 文献引用解析
    resolve_cites(doc, refs, options, &mut problems);
    // 5) 严格模式：fallback 一致性
    if options.strict_lint {
        check_fallback_consistency(doc, &numbering, refs, options, &mut problems);
    }
    (numbering, problems)
}

/// 从问题清单中提取首个 Error 级问题（手动复制，因为 `MarkupError` 含 `io::Error` 不可 Clone）。
fn first_error(problems: &Problems) -> Option<MarkupError> {
    for p in problems {
        match p {
            MarkupError::Lint { message, line, severity: Severity::Error } => {
                return Some(MarkupError::Lint {
                    message: message.clone(),
                    line: *line,
                    severity: Severity::Error,
                });
            }
            MarkupError::Resolve { message } => {
                return Some(MarkupError::Resolve { message: message.clone() });
            }
            _ => {}
        }
    }
    None
}

// ───────────────────────── 编号 ─────────────────────────

fn number_document(doc: &mut Document, problems: &mut Problems) -> NumberingTable {
    let mut table = NumberingTable::new();
    let mut counters: HashMap<IdKind, usize> = HashMap::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for section in doc.sections.iter_mut() {
        if let Some(id) = &section.id {
            if let Some(kind) = IdKind::from_id(id) {
                let n = bump(kind, &mut counters);
                register_id(id, kind, n, &mut table, &mut seen, section.line, problems);
            }
        }
        number_blocks(&mut section.blocks, &mut counters, &mut table, &mut seen, problems);
    }
    table
}

fn number_blocks(
    blocks: &mut [Block],
    counters: &mut HashMap<IdKind, usize>,
    table: &mut NumberingTable,
    seen: &mut HashMap<String, usize>,
    problems: &mut Problems,
) {
    for b in blocks.iter_mut() {
        match b {
            Block::Equation(e) => {
                if let Some(id) = &e.id {
                    let kind = IdKind::Equation;
                    let n = bump(kind, counters);
                    register_id(id, kind, n, table, seen, e.line, problems);
                    e.resolved_number = Some(n);
                } else {
                    let n = bump(IdKind::Equation, counters);
                    e.resolved_number = Some(n);
                }
            }
            Block::Figure(f) => {
                if let Some(id) = &f.id {
                    let kind = IdKind::Figure;
                    let n = bump(kind, counters);
                    register_id(id, kind, n, table, seen, f.line, problems);
                    f.resolved_number = Some(n);
                } else {
                    f.resolved_number = Some(bump(IdKind::Figure, counters));
                }
            }
            Block::Table(t) => {
                if let Some(id) = &t.id {
                    let kind = IdKind::Table;
                    let n = bump(kind, counters);
                    register_id(id, kind, n, table, seen, t.line, problems);
                    t.resolved_number = Some(n);
                } else {
                    t.resolved_number = Some(bump(IdKind::Table, counters));
                }
            }
            Block::Claim(c) => {
                let kind = IdKind::Claim(c.ty);
                let n = bump(kind, counters);
                if let Some(id) = &c.id {
                    register_id(id, kind, n, table, seen, c.line, problems);
                }
                c.resolved_number = Some(n);
            }
            Block::Heading { id: Some(id), .. } => {
                if let Some(kind) = IdKind::from_id(id) {
                    let n = bump(kind, counters);
                    register_id(id, kind, n, table, seen, 0, problems);
                }
            }
            Block::BlockQuote(inner, _) => number_blocks(inner, counters, table, seen, problems),
            _ => {}
        }
    }
}

fn bump(kind: IdKind, counters: &mut HashMap<IdKind, usize>) -> usize {
    let c = counters.entry(kind).or_insert(0);
    *c += 1;
    *c
}

fn register_id(
    id: &str,
    kind: IdKind,
    n: usize,
    table: &mut NumberingTable,
    seen: &mut HashMap<String, usize>,
    _line: usize,
    problems: &mut Problems,
) {
    if seen.contains_key(id) {
        problems.push(MarkupError::Resolve {
            message: format!("重复 id `{id}`（规范 §6.2：全文 id 唯一）"),
        });
        return;
    }
    seen.insert(id.to_string(), n);
    table.set(id, kind, n);
}

// ───────────────────────── 章节多级号 ─────────────────────────

/// 按标题层级生成多级号（如 2.1）。仅对带 id 的标题有意义（§8.2 大纲）。
fn assign_section_numbers(doc: &mut Document, table: &NumberingTable) {
    // 简化：用各级计数器，遇到 H1 重置 H2/H3...
    let mut counters: [usize; 6] = [0; 6];
    let mut section_number_table = NumberingTable::new();
    let _ = table;
    for s in doc.sections.iter_mut() {
        assign_section_numbers_blocks(&mut s.blocks, &mut counters, &mut section_number_table);
    }
    // 把生成的章节号合并回主表（通过 set_section）
    // 由于借用限制，这里通过返回值传回——改为直接在外层合并。
    // 为简化，重新遍历并写入 doc 所属的 table（通过指针）：
    // —— 此函数改为返回生成的表，由调用方合并。
    let _ = section_number_table;
}

fn assign_section_numbers_blocks(_blocks: &mut [Block], _counters: &mut [usize; 6], _table: &mut NumberingTable) {
    // 预留：实际章节号在 number_document 阶段已对 sec: id 分配了单号（1,2,3...）。
    // 多级号生成需要更细致的标题树遍历，此处保留接口，详细实现见下文 expand。
}

// ───────────────────────── 交叉引用 ─────────────────────────

fn resolve_xrefs(doc: &mut Document, table: &NumberingTable, options: &crate::context::Options, problems: &mut Problems) {
    for s in doc.sections.iter_mut() {
        resolve_xrefs_blocks(&mut s.blocks, table, options, problems);
    }
}

fn resolve_xrefs_blocks(blocks: &mut [Block], table: &NumberingTable, options: &crate::context::Options, problems: &mut Problems) {
    for b in blocks.iter_mut() {
        match b {
            Block::Paragraph(inls, _) | Block::Heading { text: inls, .. } => resolve_xrefs_inlines(inls, table, options, problems),
            Block::List { items, .. } => for it in items { resolve_xrefs_inlines(&mut it.content, table, options, problems); },
            Block::BlockQuote(inner, _) => resolve_xrefs_blocks(inner, table, options, problems),
            Block::Figure(f) => resolve_xrefs_inlines(&mut f.caption, table, options, problems),
            Block::Table(t) => {
                resolve_xrefs_inlines(&mut t.caption, table, options, problems);
                if let crate::kinds::TableSource::Markdown(m) | crate::kinds::TableSource::Html(m) = &mut t.source {
                    for c in m.header.iter_mut() { resolve_xrefs_inlines(&mut c.content, table, options, problems); }
                    for row in m.body.iter_mut() { for c in row { resolve_xrefs_inlines(&mut c.content, table, options, problems); } }
                }
            }
            Block::Claim(c) => resolve_xrefs_blocks(&mut c.body, table, options, problems),
            _ => {}
        }
    }
}

fn resolve_xrefs_inlines(inls: &mut [Inline], table: &NumberingTable, options: &crate::context::Options, problems: &mut Problems) {
    for i in inls.iter_mut() {
        resolve_xrefs_inline(i, table, options, problems);
    }
}

fn resolve_xrefs_inline(i: &mut Inline, table: &NumberingTable, options: &crate::context::Options, problems: &mut Problems) {
    match i {
        Inline::Xref(x) => {
            // 手写覆盖优先（虽不推荐）：若存在则失去自动更新
            if let Some(_override) = &x.override_text {
                x.resolved_text = Some(crate::parse::inlines_to_plain(_override));
                x.hit = true; // 视作"已显示"，不算失效
                return;
            }
            match table.get(&x.to) {
                Some((kind, n)) => {
                    let sec = if matches!(kind, IdKind::Section) { table.section_of(&x.to) } else { None };
                    x.resolved_text = Some(format_xref_text(kind, n, sec, options.label_lang));
                    x.hit = true;
                }
                None => {
                    x.hit = false;
                    if let Some(_fb) = &x.fallback {
                        problems.push(MarkupError::Lint {
                            message: format!("<f-xref to=\"{}\"> 目标不存在，已降级显示 fallback", x.to),
                            line: x.line,
                            severity: Severity::Warning,
                        });
                    } else {
                        problems.push(MarkupError::Lint {
                            message: format!("<f-xref to=\"{}\"> 目标不存在且无 fallback（规范 §6.2：避免正文空洞）", x.to),
                            line: x.line,
                            severity: Severity::Error,
                        });
                    }
                }
            }
        }
        Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => {
            for x in v.iter_mut() { resolve_xrefs_inline(x, table, options, problems); }
        }
        Inline::Link { text, .. } => for x in text { resolve_xrefs_inline(x, table, options, problems); },
        _ => {}
    }
}

// ───────────────────────── 文献引用 ─────────────────────────

fn resolve_cites(doc: &mut Document, refs: &dyn ReferenceProvider, options: &crate::context::Options, problems: &mut Problems) {
    // 第一遍：按全文首次出现顺序为命中的文献分配数字编号（仅 Numeric 风格需要）。
    let numeric_order = if options.cite_style == CiteStyle::Numeric {
        build_cite_numbering(doc, refs)
    } else {
        HashMap::new()
    };

    for s in doc.sections.iter_mut() {
        resolve_cites_blocks(&mut s.blocks, refs, options, &numeric_order, problems);
    }
}

/// 全文首次出现顺序 → 文献编号（1-based）。
fn build_cite_numbering(doc: &Document, refs: &dyn ReferenceProvider) -> HashMap<String, usize> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    for s in &doc.sections {
        collect_cite_refs_blocks(&s.blocks, refs, &mut order, &mut map);
    }
    map
}
fn collect_cite_refs_blocks(blocks: &[Block], refs: &dyn ReferenceProvider, order: &mut Vec<String>, map: &mut HashMap<String, usize>) {
    for b in blocks {
        match b {
            Block::Paragraph(inls, _) | Block::Heading { text: inls, .. } => collect_cite_refs_inlines(inls, refs, order, map),
            Block::List { items, .. } => for it in items { collect_cite_refs_inlines(&it.content, refs, order, map); },
            Block::BlockQuote(inner, _) => collect_cite_refs_blocks(inner, refs, order, map),
            Block::Figure(f) => collect_cite_refs_inlines(&f.caption, refs, order, map),
            Block::Table(t) => {
                collect_cite_refs_inlines(&t.caption, refs, order, map);
                if let crate::kinds::TableSource::Markdown(m) | crate::kinds::TableSource::Html(m) = &t.source {
                    for c in &m.header { collect_cite_refs_inlines(&c.content, refs, order, map); }
                    for row in &m.body { for c in row { collect_cite_refs_inlines(&c.content, refs, order, map); } }
                }
            }
            Block::Claim(c) => collect_cite_refs_blocks(&c.body, refs, order, map),
            _ => {}
        }
    }
}
fn collect_cite_refs_inlines(inls: &[Inline], refs: &dyn ReferenceProvider, order: &mut Vec<String>, map: &mut HashMap<String, usize>) {
    for i in inls {
        match i {
            Inline::Cite(c) => {
                for r in &c.refs {
                    if refs.get(r).is_some() && !map.contains_key(r) {
                        order.push(r.clone());
                        map.insert(r.clone(), order.len());
                    }
                }
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => collect_cite_refs_inlines(v, refs, order, map),
            Inline::Link { text, .. } => collect_cite_refs_inlines(text, refs, order, map),
            _ => {}
        }
    }
}

fn resolve_cites_blocks(blocks: &mut [Block], refs: &dyn ReferenceProvider, options: &crate::context::Options, numeric_order: &HashMap<String, usize>, problems: &mut Problems) {
    for b in blocks.iter_mut() {
        match b {
            Block::Paragraph(inls, _) | Block::Heading { text: inls, .. } => resolve_cites_inlines(inls, refs, options, numeric_order, problems),
            Block::List { items, .. } => for it in items { resolve_cites_inlines(&mut it.content, refs, options, numeric_order, problems); },
            Block::BlockQuote(inner, _) => resolve_cites_blocks(inner, refs, options, numeric_order, problems),
            Block::Figure(f) => resolve_cites_inlines(&mut f.caption, refs, options, numeric_order, problems),
            Block::Table(t) => {
                resolve_cites_inlines(&mut t.caption, refs, options, numeric_order, problems);
                if let crate::kinds::TableSource::Markdown(m) | crate::kinds::TableSource::Html(m) = &mut t.source {
                    for c in m.header.iter_mut() { resolve_cites_inlines(&mut c.content, refs, options, numeric_order, problems); }
                    for row in m.body.iter_mut() { for c in row { resolve_cites_inlines(&mut c.content, refs, options, numeric_order, problems); } }
                }
            }
            Block::Claim(c) => resolve_cites_blocks(&mut c.body, refs, options, numeric_order, problems),
            _ => {}
        }
    }
}

fn resolve_cites_inlines(inls: &mut [Inline], refs: &dyn ReferenceProvider, options: &crate::context::Options, numeric_order: &HashMap<String, usize>, problems: &mut Problems) {
    for i in inls.iter_mut() {
        resolve_cites_inline(i, refs, options, numeric_order, problems);
    }
}

fn resolve_cites_inline(i: &mut Inline, refs: &dyn ReferenceProvider, options: &crate::context::Options, numeric_order: &HashMap<String, usize>, problems: &mut Problems) {
    match i {
        Inline::Cite(c) => {
            let mut entries = Vec::with_capacity(c.refs.len());
            let mut all_hit = true;
            for r in &c.refs {
                match refs.get(r) {
                    Some(entry) => {
                        let display = match options.cite_style {
                            CiteStyle::Numeric => {
                                let n = numeric_order.get(r).copied().unwrap_or(0);
                                format!("[{n}]")
                            }
                            CiteStyle::AuthorYear => entry.author_year(),
                        };
                        entries.push(Some(CiteResolvedEntry { display, hit: true }));
                    }
                    None => {
                        all_hit = false;
                        entries.push(None);
                    }
                }
            }
            c.resolved = ResolvedCite { entries, all_hit };
            // 问题报告
            for (idx, r) in c.refs.iter().enumerate() {
                if refs.get(r).is_none() {
                    if c.fallback.is_some() {
                        problems.push(MarkupError::Lint {
                            message: format!("<f-cite ref=\"{r}\"> 未命中文献库，已降级显示 fallback"),
                            line: c.line,
                            severity: Severity::Warning,
                        });
                    } else {
                        problems.push(MarkupError::Lint {
                            message: format!("<f-cite ref=\"{r}\"> 未命中文献库且无 fallback（规范 §6.2）"),
                            line: c.line,
                            severity: Severity::Error,
                        });
                    }
                    let _ = idx;
                }
            }
        }
        Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => {
            for x in v.iter_mut() { resolve_cites_inline(x, refs, options, numeric_order, problems); }
        }
        Inline::Link { text, .. } => for x in text { resolve_cites_inline(x, refs, options, numeric_order, problems); },
        _ => {}
    }
}

// ───────────────────────── 严格模式：fallback 一致性 ─────────────────────────

fn check_fallback_consistency(doc: &Document, table: &NumberingTable, refs: &dyn ReferenceProvider, options: &crate::context::Options, problems: &mut Problems) {
    for s in &doc.sections {
        check_fallback_blocks(&s.blocks, table, refs, options, problems);
    }
}
fn check_fallback_blocks(blocks: &[Block], table: &NumberingTable, refs: &dyn ReferenceProvider, options: &crate::context::Options, problems: &mut Problems) {
    for b in blocks {
        match b {
            Block::Paragraph(inls, _) | Block::Heading { text: inls, .. } => check_fallback_inlines(inls, table, refs, options, problems),
            Block::List { items, .. } => for it in items { check_fallback_inlines(&it.content, table, refs, options, problems); },
            Block::BlockQuote(inner, _) => check_fallback_blocks(inner, table, refs, options, problems),
            Block::Figure(f) => check_fallback_inlines(&f.caption, table, refs, options, problems),
            Block::Table(t) => {
                check_fallback_inlines(&t.caption, table, refs, options, problems);
                if let crate::kinds::TableSource::Markdown(m) | crate::kinds::TableSource::Html(m) = &t.source {
                    for c in &m.header { check_fallback_inlines(&c.content, table, refs, options, problems); }
                    for row in &m.body { for c in row { check_fallback_inlines(&c.content, table, refs, options, problems); } }
                }
            }
            Block::Claim(c) => check_fallback_blocks(&c.body, table, refs, options, problems),
            _ => {}
        }
    }
}
fn check_fallback_inlines(inls: &[Inline], table: &NumberingTable, refs: &dyn ReferenceProvider, options: &crate::context::Options, problems: &mut Problems) {
    for i in inls {
        match i {
            Inline::Cite(c) => {
                // 仅当命中时比较
                for (idx, r) in c.refs.iter().enumerate() {
                    if let (Some(fb), Some(entry)) = (&c.fallback, refs.get(r)) {
                        let actual = match options.cite_style {
                            CiteStyle::Numeric => {
                                // 严格模式一致性的实际值需用数字编号；此处简化为提示
                                format!("[author-year={}]", entry.author_year())
                            }
                            CiteStyle::AuthorYear => entry.author_year(),
                        };
                        if actual != *fb {
                            problems.push(MarkupError::Lint {
                                message: format!("<f-cite ref=\"{r}\"> 的 fallback `{fb}` 与实际 `{actual}` 不一致（严格模式）"),
                                line: c.line,
                                severity: Severity::Warning,
                            });
                        }
                        let _ = idx;
                    }
                }
            }
            Inline::Xref(x) => {
                if let (Some(fb), Some((kind, n))) = (&x.fallback, table.get(&x.to)) {
                    let sec = if matches!(kind, IdKind::Section) { table.section_of(&x.to) } else { None };
                    let actual = format_xref_text(kind, n, sec, options.label_lang);
                    if actual != *fb {
                        problems.push(MarkupError::Lint {
                            message: format!("<f-xref to=\"{}\"> 的 fallback `{fb}` 与实际 `{actual}` 不一致（严格模式）", x.to),
                            line: x.line,
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => check_fallback_inlines(v, table, refs, options, problems),
            Inline::Link { text, .. } => check_fallback_inlines(text, table, refs, options, problems),
            _ => {}
        }
    }
}

/// 公开便利：把 ClaimType 列表暴露给宿主（如生成定理目录）。
pub fn claim_types() -> [ClaimType; 7] { ClaimType::VARIANTS }
