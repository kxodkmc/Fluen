//! boardgame — Rust 桌游开发框架
//!
//! 该 crate 提供卡牌类桌游开发的通用基础设施，分为以下几层：
//!
//! | 子模块 | 职责 | 状态 |
//! | :--- | :--- | :--- |
//! | **entity** | 卡牌实体抽象与标准扑克牌实现 | 本次实现 |
//! | **core** | 牌堆 / 手牌等核心数据结构，支持确定性 RNG 注入 | 本次实现 |
//! | **rule** | 策略层规则判定引擎 | 待实现 |
//! | **engine** | 状态机与事件分发 | 待实现 |
//! | **test_kit** | 测试基础设施 | 待实现 |
//!
//! 设计原则：
//! - 实体层刻意不要求 `Ord`，排序权交由下游业务决定
//! - 核心层支持注入确定性随机数生成器，便于测试与回放
//! - 不使用 `Rc` / `RefCell` / `Cell`，保持数据所有权清晰

pub mod core;
pub mod engine;
pub mod entity;
pub mod prelude;
pub mod presets;
pub mod rule;
pub mod test_kit;
