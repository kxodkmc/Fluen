# fluen-markup

论文标记规范 v1.1 的 Rust 解析与渲染 SDK。

在 Markdown 之上叠加 7 个 `f-` 前缀自定义标签，补齐论文写作所需的四项能力——**编号浮动体、稳定交叉引用、文献库绑定、编号公式/定理**。其余一律使用原生 Markdown。

## 特性

- **零外部依赖** — 默认无需任何第三方 crate；`serde` 为可选 feature
- **四阶段解耦** — 解析 → 校验 → 编号解析 → 渲染，各阶段可独立使用
- **完整 Pipeline** — `Pipeline::new(opts).parse(src).resolve().render(&html)` 一站式调用
- **双渲染器** — HTML（含默认 CSS）与纯文本
- **失效可降级** — `fallback` 属性兜底，避免数据缺失时正文空洞
- **Linter 内置** — §6.2 强制规则 + §8.5 完整性校验

## 快速开始

```rust
use fluen_markup::{Pipeline, HtmlRenderer, Options, InMemoryReferences, ReferenceEntry};

let src = r#"
## 引言 {#sec:intro}

近年来，<f-cite ref="ref-a"/> 提出的方法表现突出（见 <f-xref to="fig:f"/>）。

<f-eq id="eq:e">
e^{i\pi} + 1 = 0
</f-eq>

<f-fig id="fig:f" src="assets/f.png" alt="示例">
  <f-caption>示例图。</f-caption>
</f-fig>
"#;

let mut refs = InMemoryReferences::new();
refs.insert("ref-a", ReferenceEntry {
    authors: vec!["Chen".into()],
    year: Some("2020".into()),
    title: None,
});

let html = Pipeline::new(Options::default())
    .references(refs)
    .parse(src).unwrap()
    .resolve_or_degrade()
    .render(&HtmlRenderer::default())
    .unwrap();
```

## 标签速查

| 标签 | 类型 | 作用 | 关键属性 |
|------|------|------|----------|
| `<f-cite>` | 行内 | 文献引用 | `ref` [`loc`] [`fallback`] |
| `<f-xref>` | 行内 | 交叉引用 | `to` [`fallback`] |
| `<f-eq>` | 块级 | 编号公式 | [`id`] |
| `<f-fig>` | 块级 | 图（浮动体） | [`id`] `src` [`alt`] |
| `<f-tbl>` | 块级 | 表（浮动体） | [`id`] [`src`] [`variant`] |
| `<f-claim>` | 块级 | 定理/定义/引理… | [`id`] `type` |
| `<f-caption>` | 辅助 | 图/表题注 | 仅用于 `<f-fig>`/`<f-tbl>` 内部 |

## ID 前缀映射

| 元素 | id 前缀 | 示例 |
|------|---------|------|
| 图 | `fig:` | `fig:loss-curve` |
| 表 | `tbl:` | `tbl:results` |
| 公式 | `eq:` | `eq:euler-identity` |
| 章节 | `sec:` | `sec:method` |
| 定理 | `thm:` | `thm:convergence` |
| 引理 | `lem:` | `lem:auxiliary` |
| 定义 | `def:` | `def:metric` |
| 命题 | `prop:` | `prop:bound` |
| 推论 | `cor:` | `cor:direct` |
| 例 | `exa:` | `exa:counter` |
| 注 | `rem:` | `rem:note` |

## 架构

```
parse → validate → resolve → render
  │        │         │         │
  │        │         │         ├─ HtmlRenderer   (HTML + 默认 CSS)
  │        │         │         └─ TextRenderer   (纯文本预览)
  │        │         │
  │        │         ├─ 编号分配（图/表/式/claim 各自独立连续）
  │        │         ├─ 交叉引用解析（查表注入前缀文字）
  │        │         └─ 文献引用解析（数字制/作者-年）
  │        │
  │        ├─ §6.2 结构规则（id 唯一、前缀一致、caption 数量）
  │        └─ §8.5 引用完整性（cite/xref/fig/tbl 失效检测）
  │
  └─ f-tag 扫描 + 属性解析 + Markdown 块/行内二次解析
```

## 模块说明

| 模块 | 职责 |
|------|------|
| `lib.rs` | 公开 API：`Pipeline` / `Parsed` / `Resolved` 三阶段流水线 |
| `error.rs` | 统一错误 `MarkupError`（Parse / Lint / Resolve / Render / Io） |
| `kinds.rs` | 受控枚举：`ClaimType` / `IdKind` / `TableSource` / `TableVariant` |
| `model.rs` | AST 模型：`Document` → `Block` → `Inline`，含所有 f-* 结构体 |
| `context.rs` | 选项、文献库 trait、编号表、渲染上下文 |
| `md_inline.rs` | Markdown 行内解析（代码/数学/强调/链接/图片/实体） |
| `md_block.rs` | Markdown 块级解析（标题/段落/代码块/列表/引用/分隔线） |
| `parse.rs` | f-tag 扫描 + 属性解析 + 块/行内编排 |
| `resolve.rs` | 自动编号 + 交叉引用解析 + 文献引用解析 + 降级 |
| `validate.rs` | Linter：§6.2 结构规则 + §8.5 完整性校验 |
| `render/` | 渲染层：`Renderer` trait + `HtmlRenderer` + `TextRenderer` |
| `render/escape.rs` | HTML 转义 + URL 安全化 |
| `io.rs` | 文件读写 + 文献库 JSON 加载/导出 |

## 三阶段流水线

```rust
// 1. 解析
let parsed = Pipeline::new(options)
    .references(refs)
    .parse(src)?;

// 2. 校验（可选）
let problems = parsed.lint();

// 3a. 严格解析编号（失败返回 Err）
let resolved = parsed.resolve()?;

// 3b. 或宽松解析（失效降级为警告）
let resolved = parsed.resolve_or_degrade();

// 4. 渲染
let html = resolved.render(&HtmlRenderer::default())?;
```

## 选项

```rust
Options {
    strict_lint: false,          // 严格模式：fallback 不一致时告警
    cite_style: CiteStyle::Numeric,  // Numeric([1]) 或 AuthorYear(Smith, 2020)
    label_lang: LabelLang::Zh,   // Zh(图 1) 或 En(Figure 1)
    degrade_marker: true,        // 降级时附加 ⚠ 标记
    shared_claim_counter: false, // 定理类共享单一计数流
}
```

## 依赖哲学

零外部依赖即可全功能工作。`serde` 为唯一可选 feature，用于 AST 序列化（便于 Tauri/Electron IPC 传输）。

```toml
[features]
default = []
serde = ["dep:serde"]
```

## 文献库格式

支持 JSON 数组与对象两种形态：

```json
[
  { "id": "ref-a1b2c3d4", "authors": ["Chen", "Li"], "year": "2020", "title": "..." }
]
```

```json
{
  "ref-a1b2c3d4": { "authors": ["Chen"], "year": "2020", "title": "..." }
}
```

## 扩展渲染器

实现 `Renderer` trait 即可添加新格式：

```rust
use fluen_markup::render::Renderer;
use fluen_markup::model::Document;
use fluen_markup::context::RenderContext;

struct LatexRenderer;

impl Renderer for LatexRenderer {
    fn format_name(&self) -> &'static str { "latex" }

    fn render(&self, doc: &Document, ctx: &RenderContext<'_>) -> fluen_markup::Result<String> {
        // 你的 LaTeX 输出逻辑
    }
}
```

## 许可证

与工作区一致。
