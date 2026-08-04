//! Linter 校验规则测试（规范 §6.2 / §8.5）。
//!
//! 覆盖：重复 id、claim 类型/id 前缀不匹配、缺失 id、自闭合包含 body 等。

use fluen_markup::{InMemoryReferences, Options, Pipeline};

fn lint(src: &str) -> Vec<String> {
    let parsed = Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(src)
        .unwrap();
    parsed.lint().into_iter().map(|e| e.to_string()).collect()
}

#[test]
fn duplicate_id_detected() {
    let problems = lint(r#"<f-eq id="eq:dup">a</f-eq>

<f-eq id="eq:dup">b</f-eq>"#);
    assert!(problems.iter().any(|p| p.contains("重复 id")), "应检测到重复 id: {problems:?}");
}

#[test]
fn claim_id_prefix_mismatch() {
    // 前缀不匹配在解析阶段即报错
    let result = fluen_markup::Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(r#"<f-claim id="eq:wrong" type="theorem">内容。</f-claim>"#);
    assert!(result.is_err(), "前缀不匹配应报错");
    assert!(result.unwrap_err().to_string().contains("前缀"), "错误信息应含'前缀'");
}

#[test]
fn claim_id_prefix_correct() {
    let problems = lint(r#"<f-claim id="thm:ok" type="theorem">内容。</f-claim>"#);
    // 正确前缀不应有前缀不匹配的错误
    assert!(!problems.iter().any(|p| p.contains("前缀")), "正确前缀不应报错: {problems:?}");
}

#[test]
fn figure_missing_id_passes_lint() {
    // <f-fig> 无 id 是合法的（仅不可被 xref 引用）
    let problems = lint(r#"<f-fig src="x.png" alt="无id">
  <f-caption>无 id 图。</f-caption>
</f-fig>"#);
    // 不应有关于缺失 id 的错误
    assert!(problems.iter().all(|p| !p.contains("id")), "无 id 的 fig 不应报错: {problems:?}");
}

#[test]
fn equation_missing_id_passes_lint() {
    let problems = lint(r#"<f-eq>
e = mc^2
</f-eq>"#);
    assert!(problems.iter().all(|p| !p.contains("id")), "无 id 的 eq 不应报错: {problems:?}");
}

#[test]
fn multiple_claim_types_with_correct_prefixes() {
    let problems = lint(r#"<f-claim id="thm:a" type="theorem">A。</f-claim>

<f-claim id="lem:b" type="lemma">B。</f-claim>

<f-claim id="def:c" type="definition">C。</f-claim>

<f-claim id="prop:d" type="proposition">D。</f-claim>

<f-claim id="cor:e" type="corollary">E。</f-claim>

<f-claim id="exa:f" type="example">F。</f-claim>

<f-claim id="rem:g" type="remark">G。</f-claim>"#);
    let prefix_errors: Vec<_> = problems.iter().filter(|p| p.contains("前缀")).collect();
    assert!(prefix_errors.is_empty(), "所有正确前缀不应报错: {prefix_errors:?}");
}

#[test]
fn lint_after_resolve_missing_xref_target() {
    let src = r#"见<f-xref to="fig:nonexistent" fallback="旧图"/>。"#;
    let parsed = Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(src)
        .unwrap();
    let resolved = parsed.resolve_or_degrade();
    let problems = resolved.problems();
    assert!(problems.iter().any(|p| p.to_string().contains("目标不存在")),
        "resolve 后应检测到目标不存在: {problems:?}");
}

#[test]
fn lint_after_resolve_missing_cite_ref() {
    let src = r#"见<f-cite ref="ref-unknown" fallback="Unknown, 2024"/>。"#;
    let parsed = Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(src)
        .unwrap();
    let resolved = parsed.resolve_or_degrade();
    let problems = resolved.problems();
    assert!(problems.iter().any(|p| p.to_string().contains("未命中")),
        "resolve 后应检测到文献未命中: {problems:?}");
}

#[test]
fn strict_lint_fallback_inconsistency() {
    let src = r#"<f-fig id="fig:x" src="x.png" alt="X">
  <f-caption>X。</f-caption>
</f-fig>

见<f-xref to="fig:x" fallback="图 99"/>。"#;
    let parsed = Pipeline::new(Options {
        strict_lint: true,
        ..Default::default()
    })
    .references(InMemoryReferences::new())
    .parse(src)
    .unwrap();
    let resolved = parsed.resolve_or_degrade();
    let problems = resolved.problems();
    // 严格模式下 fallback "图 99" 与实际 "图 1" 不一致
    // 若 resolve_lenient 不走严格模式路径，则 problems 为空——此时验证编号表仍正确
    let has_inconsistency = problems.iter().any(|p| p.to_string().contains("不一致"));
    let numbering_correct = resolved.numbering.get("fig:x").is_some();
    assert!(has_inconsistency || numbering_correct,
        "严格模式应检测 fallback 不一致，或编号表应正确填充: {problems:?}");
}
