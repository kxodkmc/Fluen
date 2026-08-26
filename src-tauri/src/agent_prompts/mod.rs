//! 学术类子智能体的系统提示词 —— 角色化、可插拔的组装。
//!
//! 设计遵循 referee 哲学（数据与行为分离、命名槽位、可替换可扩展）：
//!
//! - **一角色一模块**：论文撰写 / 论文审核 / 论文思辨分别位于
//!   [`writing`]、[`review`]、[`critique`]，每个模块对外只暴露 `system()`
//!   入口，返回拼装完成的提示词字符串。
//! - **共享槽位下沉本模块**：运行环境、fluen-markup 规范速查等通用段落
//!   放这里，各角色按需复用，避免跨模块复制。
//! - **逐字内嵌不改原文**：写作角色的核心提示词（LVRV1，人类式学术写作）
//!   以 `const` 形式嵌入，版本化可控、逐字保留，外层只做引导与工具约定。
//!
//! 扩展方式：新增角色 = 新增一个子模块（实现 `system()`）+ 在
//! [`motis_chat`](crate::motis_chat) 注册对应子智能体，零改动本层其余部分。

pub(crate) mod critique;
pub(crate) mod review;
pub(crate) mod writing;

/// 运行环境：操作系统与当前日期（每次组装时动态插值）。
pub(crate) fn environment() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };
    let date = chrono::Local::now().format("%Y-%m-%d");
    format!("运行环境：\n- 平台: Fluen v0.1.0\n- 操作系统: {os}\n- 当前日期: {date}")
}

/// fluen-markup v1.1 规范速查 —— 写作角色落盘格式的唯一事实来源。
///
/// 与独立入口学术助手（`ai_assistant::prompt`）共用同一份规范文本；
/// 这里固化以供角色化子智能体复用。
pub(crate) fn markup_cheatsheet() -> &'static str {
    r#"## Fluen 论文标记规范速查（fluen-markup v1.1）

**总原则：Markdown 优先，标签按需，失效可降级。** 标题、列表、加粗、行内代码、行内公式 $...$、块公式 $$...$$、脚注、引用块全部用原生 Markdown，不加标签。

### 标签速查（共 7 个，全部 f- 前缀）

| 标签 | 作用 | 关键属性 | 示例 |
|------|------|----------|------|
| `<f-cite>` | 引用文献库 | ref（逗号分隔多篇）loc fallback | `<f-cite ref="ref-a1b2c3d4"/>` |
| `<f-xref>` | 文内交叉引用 | to fallback | `<f-xref to="fig:loss-curve"/>` |
| `<f-eq>` | 编号公式（块） | id（可选） | `<f-eq id="eq:euler">e^{i\pi}+1=0</f-eq>` |
| `<f-fig>` | 图（浮动体） | id（可选）src alt | `<f-fig id="fig:method" src="assets/x.png"><f-caption>图题</f-caption></f-fig>` |
| `<f-tbl>` | 表（浮动体） | id（可选）src variant | `<f-tbl id="tbl:results">...<f-caption>表题</f-caption></f-tbl>` |
| `<f-claim>` | 定理/定义/引理等 | type id（可选） | `<f-claim type="theorem" id="thm:conv">…</f-claim>` |
| `<f-caption>` | 图/表题注 | — | 仅用于 `<f-fig>`/`<f-tbl>` 内部 |

### ID 前缀强制映射（linter 校验，前缀不符报错）

| 元素 | id 前缀 |
|------|--------|
| 图 | `fig:` |
| 表 | `tbl:` |
| 公式 | `eq:` |
| 定理/引理/定义… | `thm:` `lem:` `def:` `prop:` `cor:` `exa:` `rem:` |

- id 用语义名（如 `fig:method-overview`），**禁止** `fig:1` 这类随重排变化的编号。
- 显示编号（图 1、表 2、式 (3)）由渲染器自动生成，**不要手写编号**。

### 关键规则

- `<f-cite>` 的 ref 必须是文献库（references-index.json）中真实存在的 id；多个引用用逗号合并：`<f-cite ref="ref-a,ref-b"/>`。
- 依赖外部数据的标签（`<f-cite>` `<f-xref>`）应提供 `fallback` 属性作后备显示，避免失效时正文空洞。
- `<f-claim>` 的 type 取固定集合：theorem / lemma / definition / proposition / corollary / example / remark。
- `<f-eq>` 仅含 LaTeX，不解析 Markdown；其余标签内部按 Markdown 二次解析。
- 章节标题保持原生 Markdown；仅当需被交叉引用时加 Pandoc 行尾 id：`## 方法 {#sec:method}`。"#
}