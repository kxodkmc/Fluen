//! Rule 层 — 接口隔离的规则判定引擎与只读上下文视图。
//!
//! 设计要点：
//! - 遵循接口隔离原则，将规则拆分为 [`ActionValidator`]（必选）、
//!   [`HandEvaluator`]（可选）、[`ValueComparator`]（可选）三个细粒度 Trait
//! - [`PlayerStatesView`] 提供各玩家公开信息的只读视野
//! - [`GameContext`] 汇总全局与局部只读状态，供规则校验与 AI 决策使用

use crate::engine::{Action, Phase};
use crate::entity::CardEntity;
use std::collections::HashMap;
use std::fmt::Debug;

/// 全局玩家状态只读视图：提供各玩家公开信息（视野层）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerStatesView {
    /// 各玩家剩余手牌数
    pub hands_count: HashMap<u32, usize>,
    /// 是否弃牌/托管
    pub is_folded: HashMap<u32, bool>,
    /// 玩家分数
    pub scores: HashMap<u32, i32>,
}

/// 游戏上下文快照：包含只读的全局与局部状态。
///
/// `history` 为引擎提供的滑动窗口切片；`current_hand` 在当前玩家不存在时
/// 安全回退为空切片。
pub struct GameContext<'a, C: CardEntity> {
    pub current_player: u32,
    pub phase: Phase,
    /// 历史滑动窗口
    pub history: &'a [Action],
    pub table_cards: &'a [C],
    pub current_hand: &'a [C],
    pub players_state: &'a PlayerStatesView,
    /// 已知打出的牌（用于记牌器）
    pub played_cards: &'a [C],
}

/// 结构化的牌型估值
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandValue {
    pub rank: u32,
    pub primary_value: u32,
    pub kickers: Vec<u32>,
}

/// 1. 动作校验器（所有游戏必须实现）
pub trait ActionValidator<C: CardEntity> {
    fn validate(&self, ctx: &GameContext<'_, C>, action: &Action) -> bool;
}

/// 2. 牌型评估器（仅需要比牌的游戏实现，如斗地主、德扑）
pub trait HandEvaluator<C: CardEntity> {
    fn evaluate(&self, ctx: &GameContext<'_, C>, cards: &[C]) -> Option<HandValue>;
}

/// 3. 牌值比较器
pub trait ValueComparator {
    fn compare(&self, val_a: &HandValue, val_b: &HandValue) -> std::cmp::Ordering;
}
