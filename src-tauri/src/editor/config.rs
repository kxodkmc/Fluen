//! 编辑器配置。

use fluen_markup::Options;

/// 编辑器配置。
#[derive(Debug, Clone)]
pub struct EditorConfig {
    /// 单栈最大记录数，默认 100。
    pub max_history: usize,
    /// 是否记录自动优化操作，默认 true。
    pub record_auto_optimize: bool,
    /// fluen-markup 渲染选项。
    pub render_options: Options,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            max_history: 100,
            record_auto_optimize: true,
            render_options: Options::default(),
        }
    }
}
