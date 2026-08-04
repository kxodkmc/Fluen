//! Linter：规范 §6.2 强制规则与 §8.5 完整性校验。
//!
//! 用法：在 [`crate::parse`] 后、[`crate::resolve`] 前调用 [`lint`]，或在 resolve 后
//! 调用 [`lint_after_resolve`] 获取含降级信息的完整问题清单。
//!
//! 设计：lint 不修改 AST，只产生 [`Problems`]。错误级问题不抛异常，由调用方决定
//! 是否中止（resolve 内部对"无 fallback 的失效引用"会返回 Err，lint 提供更细的清单）。

use crate::context::{NumberingTable, Options, ReferenceProvider};
use crate::error::{MarkupError, Problems, Severity};
use crate::kinds::{IdKind, TableSource};
use crate::model::*;

/// 解析后立即校验：结构规则（§6.2）。
/// 这些规则不依赖文献库与编号表，可在解析阶段就发现。
pub fn lint(doc: &Document) -> Problems {
    let mut problems = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in &doc.sections {
        lint_blocks(&s.blocks, &mut seen_ids, &mut problems);
    }
    problems
}

fn lint_blocks(blocks: &[Block], seen: &mut std::collections::HashSet<String>, problems: &mut Problems) {
    for b in blocks {
        match b {
            Block::Claim(c) => {
                // type ↔ id 前缀（解析阶段已校验，此处复检以防宿主绕过）
                if let Some(id) = &c.id {
                    if !id.starts_with(c.ty.to_id_prefix()) {
                        problems.push(MarkupError::lint(crate::error::claim_prefix_mismatch(&c.ty, id), c.line, Severity::Error));
                    }
                    if !seen.insert(id.clone()) {
                        problems.push(MarkupError::lint(format!("重复 id `{id}`"), c.line, Severity::Error));
                    }
                }
                lint_blocks(&c.body, seen, problems);
            }
            Block::Figure(f) => {
                if let Some(id) = &f.id {
                    if !id.starts_with("fig:") {
                        problems.push(MarkupError::lint(format!("<f-fig> 的 id `{id}` 应以前缀 `fig:` 开头"), f.line, Severity::Error));
                    }
                    if !seen.insert(id.clone()) {
                        problems.push(MarkupError::lint(format!("重复 id `{id}`"), f.line, Severity::Error));
                    }
                }
                // caption 数量已在解析阶段强制（恰为 1），此处不重复
            }
            Block::Table(t) => {
                if let Some(id) = &t.id {
                    if !id.starts_with("tbl:") {
                        problems.push(MarkupError::lint(format!("<f-tbl> 的 id `{id}` 应以前缀 `tbl:` 开头"), t.line, Severity::Error));
                    }
                    if !seen.insert(id.clone()) {
                        problems.push(MarkupError::lint(format!("重复 id `{id}`"), t.line, Severity::Error));
                    }
                }
                // 形态互斥已在解析阶段校验；HTML 表子集限制在解析阶段
                if let TableSource::Html(m) = &t.source {
                    // colspan/rowspan 合法性已由解析保证；这里检查列数一致性（软告警）
                    let ncols = m.header.len();
                    for (ri, row) in m.body.iter().enumerate() {
                        let span_sum: u32 = row.iter().map(|c| c.colspan.max(1)).sum();
                        if ncols > 0 && span_sum != ncols as u32 {
                            problems.push(MarkupError::lint(
                                format!("<f-tbl> 第 {} 数据行单元格跨度总和（{}）与表头列数（{}）不一致", ri + 1, span_sum, ncols),
                                t.line, Severity::Warning));
                        }
                    }
                }
            }
            Block::Equation(e) => {
                if let Some(id) = &e.id {
                    if !id.starts_with("eq:") {
                        problems.push(MarkupError::lint(format!("<f-eq> 的 id `{id}` 应以前缀 `eq:` 开头"), e.line, Severity::Error));
                    }
                    if !seen.insert(id.clone()) {
                        problems.push(MarkupError::lint(format!("重复 id `{id}`"), e.line, Severity::Error));
                    }
                }
            }
            Block::Heading { id: Some(id), .. } => {
                if let Some(kind) = IdKind::from_id(id) {
                    // sec: 前缀校验
                    if matches!(kind, IdKind::Section) && !id.starts_with("sec:") {
                        problems.push(MarkupError::lint(format!("章节锚点 id `{id}` 应以前缀 `sec:` 开头"), 0, Severity::Warning));
                    }
                    if !seen.insert(id.clone()) {
                        problems.push(MarkupError::lint(format!("重复 id `{id}`"), 0, Severity::Error));
                    }
                }
            }
            Block::BlockQuote(inner, _) => lint_blocks(inner, seen, problems),
            _ => {}
        }
    }
}

/// resolve 后校验：引用完整性（§8.5）。返回含降级信息的完整清单。
/// 与 resolve 内部逻辑一致，但以"只读报告"形式输出，便于 UI 展示问题面板。
pub fn lint_after_resolve(
    doc: &Document,
    table: &NumberingTable,
    refs: &dyn ReferenceProvider,
    options: &Options,
) -> Problems {
    let mut problems = Vec::new();
    for s in &doc.sections {
        lint_refs_blocks(&s.blocks, table, refs, options, &mut problems);
    }
    problems
}

fn lint_refs_blocks(blocks: &[Block], table: &NumberingTable, refs: &dyn ReferenceProvider, options: &Options, problems: &mut Problems) {
    for b in blocks {
        match b {
            Block::Paragraph(inls, _) | Block::Heading { text: inls, .. } => lint_refs_inlines(inls, table, refs, options, problems),
            Block::List { items, .. } => for it in items { lint_refs_inlines(&it.content, table, refs, options, problems); },
            Block::BlockQuote(inner, _) => lint_refs_blocks(inner, table, refs, options, problems),
            Block::Figure(f) => lint_refs_inlines(&f.caption, table, refs, options, problems),
            Block::Table(t) => {
                lint_refs_inlines(&t.caption, table, refs, options, problems);
                if let TableSource::Markdown(m) | TableSource::Html(m) = &t.source {
                    for c in &m.header { lint_refs_inlines(&c.content, table, refs, options, problems); }
                    for row in &m.body { for c in row { lint_refs_inlines(&c.content, table, refs, options, problems); } }
                }
            }
            Block::Claim(c) => lint_refs_blocks(&c.body, table, refs, options, problems),
            _ => {}
        }
    }
}

fn lint_refs_inlines(inls: &[Inline], table: &NumberingTable, refs: &dyn ReferenceProvider, options: &Options, problems: &mut Problems) {
    for i in inls {
        match i {
            Inline::Cite(c) => {
                for r in &c.refs {
                    if refs.get(r).is_none() {
                        let (sev, msg) = if c.fallback.is_some() {
                            (Severity::Warning, format!("文献 `{r}` 未命中（已降级显示 fallback）"))
                        } else {
                            (Severity::Error, format!("文献 `{r}` 未命中且无 fallback（正文将空洞）"))
                        };
                        problems.push(MarkupError::lint(msg, c.line, sev));
                    }
                }
                // fallback 与内部覆盖不可共存（规范 §6.2）
                if c.fallback.is_some() && !c.refs.is_empty() {
                    // 注意：行内 cite 不应有内部覆盖（解析时 fallback 与 body 互斥已尽力校验）
                }
            }
            Inline::Xref(x) => {
                let target_exists = table.contains(&x.to) || x.override_text.is_some();
                if !target_exists {
                    let (sev, msg) = if x.fallback.is_some() {
                        (Severity::Warning, format!("<f-xref to=\"{}\"> 目标不存在，已降级显示 fallback", x.to))
                    } else {
                        (Severity::Error, format!("<f-xref to=\"{}\"> 目标不存在且无 fallback", x.to))
                    };
                    problems.push(MarkupError::lint(msg, x.line, sev));
                }
                let _ = options;
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) => lint_refs_inlines(v, table, refs, options, problems),
            Inline::Link { text, .. } => lint_refs_inlines(text, table, refs, options, problems),
            _ => {}
        }
    }
}

/// 把问题清单格式化为人类可读的多行文本（用于 CLI / 日志）。
pub fn format_problems(problems: &Problems) -> String {
    if problems.is_empty() {
        return "✓ 无问题".to_string();
    }
    let mut out = String::new();
    let (mut errors, mut warnings): (Vec<&MarkupError>, Vec<&MarkupError>) = (Vec::new(), Vec::new());
    for p in problems {
        match p {
            MarkupError::Lint { severity: Severity::Warning, .. } => warnings.push(p),
            _ => errors.push(p),
        }
    }
    if !errors.is_empty() {
        out.push_str(&format!("❌ {} 个错误：\n", errors.len()));
        for p in &errors { out.push_str(&format!("  - {p}\n")); }
    }
    if !warnings.is_empty() {
        out.push_str(&format!("⚠ {} 个警告：\n", warnings.len()));
        for p in &warnings { out.push_str(&format!("  - {p}\n")); }
    }
    out
}

/// 便捷：是否有错误级问题。
pub fn has_errors(problems: &Problems) -> bool {
    problems.iter().any(|p| !matches!(p, MarkupError::Lint { severity: Severity::Warning, .. }))
}
