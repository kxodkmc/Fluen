//! EditorEngine——编辑器核心门面。
//!
//! 维护最小状态（source_md + dirty + history + config + project_path），
//! 提供文本替换、撤销/重做、渲染、保存等接口。
//! MD 是唯一真相源，HTML 是只读派生产物。

use std::ops::Range;

use super::config::EditorConfig;
use super::edit::{Edit, EditLabel, EditSummary};
use super::error::{EditorError, Result};
use super::history::History;
use super::render::render_to_html;

/// 编辑器引擎——核心门面。
#[derive(Debug)]
pub struct EditorEngine {
    /// 当前 MD 全文（唯一真相源）。
    source_md: String,
    /// 自上次保存后是否有未持久化变更。
    dirty: bool,
    /// 撤销/重做历史栈。
    history: History,
    /// 引擎配置。
    config: EditorConfig,
    /// 绑定的项目路径（用于委托 project 模块保存）。
    project_path: Option<String>,
}

impl EditorEngine {
    /// 创建空引擎。
    pub fn new(config: EditorConfig) -> Self {
        let max_history = config.max_history;
        Self {
            source_md: String::new(),
            dirty: false,
            history: History::new(max_history),
            config,
            project_path: None,
        }
    }

    /// 从文本加载（不产生历史记录）。
    pub fn from_text(md: impl Into<String>, config: EditorConfig) -> Self {
        let max_history = config.max_history;
        Self {
            source_md: md.into(),
            dirty: false,
            history: History::new(max_history),
            config,
            project_path: None,
        }
    }

    /// 获取当前 MD 全文。
    pub fn get_text(&self) -> &str {
        &self.source_md
    }

    /// 是否有未保存变更。
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 绑定项目路径（用于 save 委托）。
    pub fn bind_project(&mut self, project_path: impl Into<String>) {
        self.project_path = Some(project_path.into());
    }

    /// 返回当前绑定的项目路径（切片）。
    pub fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
    }

    /// 文本区间替换。
    ///
    /// `range` 基于 UTF-8 字节偏移。越界返回错误。
    pub fn replace_text(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        label: EditLabel,
    ) -> Result<()> {
        // 校验 range
        if range.start > self.source_md.len() || range.end > self.source_md.len() {
            return Err(EditorError::RangeOutOfBounds(range));
        }

        let before = self.source_md.get(range.clone()).unwrap_or("").to_string();
        let after = new_text.to_string();

        // 记录历史
        let edit = Edit::text(range.clone(), before.clone(), after.clone(), label);
        self.history.push(edit);

        // 应用替换
        let mut new_source =
            String::with_capacity(self.source_md.len() - before.len() + after.len());
        new_source.push_str(&self.source_md[..range.start]);
        new_source.push_str(&after);
        new_source.push_str(&self.source_md[range.end..]);
        self.source_md = new_source;
        self.dirty = true;

        Ok(())
    }

    /// 记录文件删除操作。
    ///
    /// 不影响 source_md，仅记录到历史栈供撤销时恢复。
    pub fn record_file_delete(
        &mut self,
        path: impl Into<std::path::PathBuf>,
        content_before: impl Into<String>,
    ) {
        let edit = Edit::file_delete(path.into(), content_before.into());
        self.history.push(edit);
        self.dirty = true;
    }

    /// 记录自动优化操作（全文替换）。
    ///
    /// 根据 config.record_auto_optimize 决定是否记录。
    /// source_md 更新为 after，dirty=true。
    pub fn record_auto_optimize(
        &mut self,
        before: impl Into<String>,
        after: impl Into<String>,
        description: impl Into<String>,
    ) {
        let before = before.into();
        let after = after.into();
        let description = description.into();

        if self.config.record_auto_optimize {
            let edit = Edit::auto_optimize(before.clone(), after.clone(), description);
            self.history.push(edit);
        }

        self.source_md = after;
        self.dirty = true;
    }

    /// 撤销。返回 true 表示成功撤销，false 表示无可撤销。
    pub fn undo(&mut self) -> Result<bool> {
        let edit = match self.history.pop_undo() {
            Some(e) => e,
            None => return Ok(false),
        };
        self.source_md = edit.revert(&self.source_md);
        self.dirty = true;
        Ok(true)
    }

    /// 重做。返回 true 表示成功重做，false 表示无可重做。
    pub fn redo(&mut self) -> Result<bool> {
        let edit = match self.history.pop_redo() {
            Some(e) => e,
            None => return Ok(false),
        };
        self.source_md = edit.apply(&self.source_md);
        self.dirty = true;
        Ok(true)
    }

    /// 清空历史栈。
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// 渲染当前 source_md 为 HTML。
    pub fn render_html(&self) -> Result<String> {
        render_to_html(&self.source_md, &self.config.render_options)
    }

    /// 渲染任意 MD 文本为 HTML（使用引擎当前配置）。
    ///
    /// 不修改 source_md，仅用于即时预览外部内容。
    pub fn render_html_with(&self, content: &str) -> Result<String> {
        render_to_html(content, &self.config.render_options)
    }

    /// 保存到 main.md（委托 project 模块）。
    ///
    /// 成功后 dirty 置为 false。
    pub fn save(&mut self) -> Result<()> {
        let project_path = self
            .project_path
            .clone()
            .ok_or(EditorError::NoProjectBound)?;

        let request = crate::project::model::SaveDocumentRequest {
            project_path,
            content: self.source_md.clone(),
        };

        crate::project::section::save_document(request)
            .map_err(|e| EditorError::ProjectError(e.to_string()))?;

        self.dirty = false;
        Ok(())
    }

    /// 保存指定内容到 main.md 并拆分回各章节备份文件（委托 project 模块）。
    ///
    /// 与 [`save`](Self::save) 的区别：接受外部 `content` 而非使用 source_md，
    /// 并返回重新加载后的 [`OpenProjectResult`]（含最新章节列表与 main_md）。
    /// 成功后 source_md 更新为返回的 main_md，dirty 置为 false。
    pub fn save_content(
        &mut self,
        content: String,
    ) -> Result<crate::project::model::OpenProjectResult> {
        let project_path = self
            .project_path
            .clone()
            .ok_or(EditorError::NoProjectBound)?;

        let request = crate::project::model::SaveDocumentRequest {
            project_path,
            content,
        };

        let result = crate::project::section::save_document(request)
            .map_err(|e| EditorError::ProjectError(e.to_string()))?;

        self.source_md = result.main_md.clone();
        self.dirty = false;
        Ok(result)
    }

    /// 从项目加载 main.md（委托 project 模块）。
    ///
    /// 加载后 dirty=false，历史栈清空。
    pub fn load_project(&mut self, project_path: &str) -> Result<()> {
        let result = crate::project::loader::open_project(project_path)
            .map_err(|e| EditorError::ProjectError(e.to_string()))?;

        self.source_md = result.main_md;
        self.project_path = Some(project_path.to_string());
        self.dirty = false;
        self.history.clear();
        Ok(())
    }

    /// 返回 (undo_count, redo_count)。
    pub fn history_len(&self) -> (usize, usize) {
        self.history.len()
    }

    /// 是否可撤销。
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// 是否可重做。
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 返回最近 n 条历史摘要。
    pub fn history_preview(&self, n: usize) -> Vec<EditSummary> {
        self.history.preview(n)
    }

    /// 获取配置引用。
    pub fn config(&self) -> &EditorConfig {
        &self.config
    }

    /// 保存资源文件到项目的 `manuscript/assets/` 目录。
    ///
    /// 关联函数（不依赖引擎实例状态），对文件名做安全校验：
    /// 禁止空名、路径分隔符与 `..`，防止路径穿越。
    /// 成功返回相对路径 `assets/{filename}`（供 MD 引用）。
    pub fn save_asset(
        project_path: String,
        filename: String,
        bytes: Vec<u8>,
    ) -> Result<String> {
        use std::path::Path;

        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
            || filename.contains(std::path::MAIN_SEPARATOR)
        {
            return Err(EditorError::InvalidAssetFilename(filename));
        }

        let assets_dir = Path::new(&project_path)
            .join("manuscript")
            .join("assets");
        std::fs::create_dir_all(&assets_dir)
            .map_err(|e| EditorError::AssetIoFailed(e.to_string()))?;

        let file_path = assets_dir.join(&filename);
        std::fs::write(&file_path, &bytes)
            .map_err(|e| EditorError::AssetIoFailed(e.to_string()))?;

        Ok(format!("assets/{}", filename))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::config::EditorConfig;
    use crate::project::creator;
    use crate::project::model::{sanitize_project_name, CreateProjectRequest};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 辅助：构造默认配置的引擎并加载文本。
    fn make_engine(md: &str) -> EditorEngine {
        EditorEngine::from_text(md, EditorConfig::default())
    }

    /// 辅助：创建唯一临时目录（遵循项目测试约定：temp_dir + pid + nanos）。
    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_engine_test_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 辅助：通过 creator 创建测试项目，返回项目根路径。
    fn create_test_project(storage: &Path) -> PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        let path = creator::create_project(request).unwrap();
        PathBuf::from(path)
    }

    /// 辅助：向项目添加章节（body 含 H1 标题行），并更新 sections.json。
    fn add_section(project_dir: &Path, id: &str, order: u32, title: &str, body: &str) {
        let sections_dir = project_dir.join("manuscript").join("sections");
        let content = format!(
            "---\ntitle: \"{}\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n{}",
            title, body
        );
        fs::write(sections_dir.join(format!("{}.md", id)), content).unwrap();

        let json_path = sections_dir.join("sections.json");
        let mut sections: Vec<serde_json::Value> =
            serde_json::from_reader(fs::File::open(&json_path).unwrap()).unwrap();
        sections.push(serde_json::json!({
            "id": id,
            "order": order,
            "title": title,
            "references": []
        }));
        fs::write(&json_path, serde_json::to_string_pretty(&sections).unwrap()).unwrap();

        // 模拟旧版项目：无 main.md（creator 会创建空 main.md，此处移除以触发迁移拼装）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));
    }

    // 1. new 创建空引擎
    #[test]
    fn new_creates_empty_engine() {
        let engine = EditorEngine::new(EditorConfig::default());
        assert_eq!(engine.get_text(), "");
        assert!(!engine.is_dirty());
        assert_eq!(engine.history_len(), (0, 0));
        assert!(!engine.can_undo());
        assert!(!engine.can_redo());
    }

    // 2. from_text 加载文本
    #[test]
    fn from_text_loads_content() {
        let engine = make_engine("# 标题\n\n正文");
        assert_eq!(engine.get_text(), "# 标题\n\n正文");
        assert!(!engine.is_dirty());
        assert_eq!(engine.history_len(), (0, 0));
    }

    // 3. replace_text 正常替换
    #[test]
    fn replace_text_updates_source_and_history() {
        let mut engine = make_engine("hello world");
        // "world" 在字节 6..11
        engine
            .replace_text(6..11, "rust", EditLabel::User)
            .unwrap();

        assert_eq!(engine.get_text(), "hello rust");
        assert!(engine.is_dirty());
        assert_eq!(engine.history_len(), (1, 0));
        assert!(engine.can_undo());
        assert!(!engine.can_redo());
    }

    // 4. replace_text 越界返回 RangeOutOfBounds
    #[test]
    fn replace_text_out_of_bounds_returns_error() {
        let mut engine = make_engine("abc");
        // 长度 3，访问 2..10 越界
        let result = engine.replace_text(2..10, "x", EditLabel::User);
        assert!(matches!(result, Err(EditorError::RangeOutOfBounds(_))));

        // source_md 不应变
        assert_eq!(engine.get_text(), "abc");
        // 历史不应记录
        assert_eq!(engine.history_len(), (0, 0));
        assert!(!engine.is_dirty());
    }

    // 5. replace_text 零长度插入（range n..n）
    #[test]
    fn replace_text_zero_length_insertion() {
        let mut engine = make_engine("hello world");
        // 在位置 6 插入 "new "（range 6..6，before=""）
        engine
            .replace_text(6..6, "new ", EditLabel::User)
            .unwrap();

        assert_eq!(engine.get_text(), "hello new world");
        assert!(engine.is_dirty());
        assert_eq!(engine.history_len(), (1, 0));
    }

    // 6. undo / redo 往返
    #[test]
    fn undo_redo_roundtrip() {
        let mut engine = make_engine("hello world");
        let original = engine.get_text().to_string();

        // 替换
        engine
            .replace_text(6..11, "rust", EditLabel::User)
            .unwrap();
        assert_eq!(engine.get_text(), "hello rust");

        // 撤销 → 恢复原文
        let undone = engine.undo().unwrap();
        assert!(undone);
        assert_eq!(engine.get_text(), original);

        // 重做 → 恢复替换
        let redone = engine.redo().unwrap();
        assert!(redone);
        assert_eq!(engine.get_text(), "hello rust");
    }

    // 7. undo 空栈返回 Ok(false)
    #[test]
    fn undo_empty_stack_returns_false() {
        let mut engine = make_engine("hello");
        let result = engine.undo().unwrap();
        assert!(!result);
        assert_eq!(engine.get_text(), "hello");
    }

    // 8. redo 空栈返回 Ok(false)
    #[test]
    fn redo_empty_stack_returns_false() {
        let mut engine = make_engine("hello");
        let result = engine.redo().unwrap();
        assert!(!result);
    }

    // 9. clear_history 清空栈
    #[test]
    fn clear_history_empties_stacks() {
        let mut engine = make_engine("hello world");
        engine
            .replace_text(0..5, "hi", EditLabel::User)
            .unwrap();
        assert_eq!(engine.history_len(), (1, 0));

        engine.clear_history();
        assert_eq!(engine.history_len(), (0, 0));
        assert!(!engine.can_undo());
        assert!(!engine.can_redo());
    }

    // 10. history_len / can_undo / can_redo 查询正确
    #[test]
    fn history_queries_are_correct() {
        let mut engine = make_engine("hello world");

        // 初始状态
        assert!(!engine.can_undo());
        assert!(!engine.can_redo());
        assert_eq!(engine.history_len(), (0, 0));

        // push 一条
        engine
            .replace_text(0..5, "hi", EditLabel::User)
            .unwrap();
        assert!(engine.can_undo());
        assert!(!engine.can_redo());
        assert_eq!(engine.history_len(), (1, 0));

        // 撤销后
        engine.undo().unwrap();
        assert!(!engine.can_undo());
        assert!(engine.can_redo());
        assert_eq!(engine.history_len(), (0, 1));

        // 重做后
        engine.redo().unwrap();
        assert!(engine.can_undo());
        assert!(!engine.can_redo());
        assert_eq!(engine.history_len(), (1, 0));
    }

    // 11. history_preview(n) 返回摘要
    #[test]
    fn history_preview_returns_summaries() {
        let mut engine = make_engine("hello world");

        engine
            .replace_text(0..5, "hi", EditLabel::User)
            .unwrap();
        // 第一次替换后文本变为 "hi world"（长度 8），"world" 移到 3..8
        engine
            .replace_text(3..8, "rust", EditLabel::User)
            .unwrap();

        let previews = engine.history_preview(10);
        assert_eq!(previews.len(), 2);
        // 栈顶在前：最近一条是 "world" → "rust"
        assert_eq!(previews[0].kind, "text");
        assert!(previews[0].description.contains("rust"));
        assert!(previews[1].description.contains("hi"));

        // 限制 n
        let one = engine.history_preview(1);
        assert_eq!(one.len(), 1);
        assert!(one[0].description.contains("rust"));
    }

    // 12. record_auto_optimize 更新 source_md 且记录历史
    #[test]
    fn record_auto_optimize_updates_and_records() {
        let mut engine = make_engine("# 原始内容\n\n正文");

        engine.record_auto_optimize(
            "# 原始内容\n\n正文",
            "# 优化后\n\n格式化正文",
            "全文格式化",
        );

        assert_eq!(engine.get_text(), "# 优化后\n\n格式化正文");
        assert!(engine.is_dirty());
        assert_eq!(engine.history_len(), (1, 0));
        assert!(engine.can_undo());

        // 撤销应恢复到 before
        engine.undo().unwrap();
        assert_eq!(engine.get_text(), "# 原始内容\n\n正文");
    }

    // 12b. record_auto_optimize 在 record_auto_optimize=false 时不记录历史
    #[test]
    fn record_auto_optimize_respects_config_flag() {
        let mut config = EditorConfig::default();
        config.record_auto_optimize = false;
        let mut engine = EditorEngine::from_text("原文", config);

        engine.record_auto_optimize("原文", "新文", "描述");

        // source_md 仍更新
        assert_eq!(engine.get_text(), "新文");
        assert!(engine.is_dirty());
        // 但历史栈为空
        assert_eq!(engine.history_len(), (0, 0));
        assert!(!engine.can_undo());
    }

    // 13. record_file_delete 不影响 source_md 但记录历史且 dirty=true
    #[test]
    fn record_file_delete_does_not_affect_source() {
        let mut engine = make_engine("正文不变");
        let original = engine.get_text().to_string();

        engine.record_file_delete("/tmp/foo.md", "文件原内容");

        // source_md 不变
        assert_eq!(engine.get_text(), original);
        // dirty 置 true
        assert!(engine.is_dirty());
        // 历史记录一条
        assert_eq!(engine.history_len(), (1, 0));
        assert!(engine.can_undo());

        // 摘要类型正确
        let previews = engine.history_preview(1);
        assert_eq!(previews[0].kind, "file_delete");
        assert!(previews[0].description.contains("foo.md"));

        // 撤销后 source_md 仍不变（FileDelete revert 返回原文本）
        engine.undo().unwrap();
        assert_eq!(engine.get_text(), original);
    }

    // 14. bind_project 绑定路径
    #[test]
    fn bind_project_sets_path() {
        let mut engine = make_engine("hello");
        // 未绑定时 save 应返回 NoProjectBound
        let result = engine.save();
        assert!(matches!(result, Err(EditorError::NoProjectBound)));

        // 绑定后不再返回 NoProjectBound（实际 save 会因路径不存在而失败，
        // 但错误类型应为 ProjectError 而非 NoProjectBound）
        engine.bind_project("/nonexistent/path");
        let result = engine.save();
        // 路径不存在，应返回 ProjectError（不再是 NoProjectBound）
        assert!(!matches!(result, Err(EditorError::NoProjectBound)));
    }

    // 15. save 未绑定项目返回 NoProjectBound 错误
    #[test]
    fn save_without_project_returns_no_project_bound() {
        let mut engine = make_engine("一些内容");
        let result = engine.save();
        assert!(matches!(result, Err(EditorError::NoProjectBound)));
        // 保存失败时 dirty 不应变（make_engine 初始 dirty=false）
        assert!(!engine.is_dirty());
        // 由于未保存成功，source_md 不变
        assert_eq!(engine.get_text(), "一些内容");
    }

    // 额外：render_html 渲染当前内容
    #[test]
    fn render_html_renders_current_source() {
        let engine = make_engine("# 标题\n\n正文");
        let html = engine.render_html().unwrap();
        assert!(html.contains("<h1"));
        assert!(html.contains("标题"));
    }

    // 额外：config() 返回配置引用
    #[test]
    fn config_returns_reference() {
        let engine = EditorEngine::new(EditorConfig::default());
        let config = engine.config();
        assert_eq!(config.max_history, 100);
        assert!(config.record_auto_optimize);
    }

    // 额外：多次 replace_text 后 undo 链能依次回退
    #[test]
    fn multiple_edits_undo_in_reverse_order() {
        let mut engine = make_engine("abc");
        let original = engine.get_text().to_string();

        engine.replace_text(0..0, "x", EditLabel::User).unwrap(); // "xabc"
        engine.replace_text(1..1, "y", EditLabel::User).unwrap(); // "xyabc"
        engine.replace_text(2..2, "z", EditLabel::User).unwrap(); // "xyzabc"
        assert_eq!(engine.get_text(), "xyzabc");
        assert_eq!(engine.history_len(), (3, 0));

        // 依次撤销
        engine.undo().unwrap();
        assert_eq!(engine.get_text(), "xyabc");
        engine.undo().unwrap();
        assert_eq!(engine.get_text(), "xabc");
        engine.undo().unwrap();
        assert_eq!(engine.get_text(), original);

        assert_eq!(engine.history_len(), (0, 3));
        assert!(engine.can_redo());
    }

    // ── render_html_with ──

    // render_html_with 渲染任意内容（不影响 source_md）
    #[test]
    fn render_html_with_renders_provided_content() {
        let engine = make_engine("# Hello\nworld");
        let html = engine.render_html_with("# Test\n\nparagraph").unwrap();
        assert!(html.contains("<h1"));
        assert!(html.contains("Test"));
        // source_md 不变
        assert_eq!(engine.get_text(), "# Hello\nworld");
    }

    // render_html_with 空字符串不 panic
    #[test]
    fn render_html_with_empty_string_does_not_panic() {
        let engine = make_engine("anything");
        let result = engine.render_html_with("");
        assert!(result.is_ok());
    }

    // ── project_path ──

    // project_path 未绑定时返回 None，绑定后返回 Some
    #[test]
    fn project_path_accessor_reflects_binding() {
        let mut engine = make_engine("hello");
        assert!(engine.project_path().is_none());

        engine.bind_project("/some/path");
        assert_eq!(engine.project_path(), Some("/some/path"));
    }

    // ── save_content ──

    // save_content 写入 main.md 并拆分回章节备份文件
    #[test]
    fn save_content_writes_main_md_and_sections() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        let project_path = project_dir.to_str().unwrap().to_string();
        let mut engine = EditorEngine::new(EditorConfig::default());
        engine.load_project(&project_path).unwrap();

        // 修改标题
        let modified = engine.get_text().replace("# 引言", "# 绪论");
        let result = engine.save_content(modified.clone()).unwrap();

        // 返回的 main_md 含修改后内容
        assert!(result.main_md.contains("# 绪论"));
        assert!(!result.main_md.contains("# 引言"));
        // 章节标题已更新
        assert_eq!(result.sections[0].title, "绪论");

        // main.md 文件存在且含修改内容
        let main_path = project_dir.join("manuscript").join("main.md");
        assert!(main_path.exists());
        let main_file = fs::read_to_string(&main_path).unwrap();
        assert!(main_file.contains("# 绪论"));

        // sec 备份文件已更新
        let sec_file = fs::read_to_string(
            project_dir.join("manuscript").join("sections").join("sec-aaa11111.md"),
        )
        .unwrap();
        assert!(sec_file.contains("# 绪论"));
        assert!(!sec_file.contains("# 引言"));

        // 引擎状态：dirty=false，source_md 同步为返回的 main_md
        assert!(!engine.is_dirty());
        assert_eq!(engine.get_text(), result.main_md);

        let _ = fs::remove_dir_all(&storage);
    }

    // save_content 未绑定项目返回 NoProjectBound
    #[test]
    fn save_content_without_project_returns_no_project_bound() {
        let mut engine = make_engine("一些内容");
        let result = engine.save_content("新内容".into());
        assert!(matches!(result, Err(EditorError::NoProjectBound)));
        // source_md 不变
        assert_eq!(engine.get_text(), "一些内容");
    }
}
