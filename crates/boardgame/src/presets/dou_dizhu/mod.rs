//! 斗地主预设 — 基于主流规则的开箱即用斗地主组件。
//!
//! 模块化拆分：每个核心组件（牌型识别/评估器/比较器/校验器/叫地主/计分/副作用/配置）
//! 均为独立可替换类型，下游可按需组合或替换。
//!
//! # 快速开始
//! ```ignore
//! use boardgame::presets::dou_dizhu::prelude::*;
//! ```

pub mod bidding;
pub mod card;
pub mod comparator;
pub mod effects;
pub mod evaluator;
pub mod pattern;
pub mod scoring;
pub mod setup;
pub mod validator;

#[cfg(test)]
mod tests;

/// 预导入：斗地主常用类型一键可用。
pub mod prelude {
    pub use super::bidding::{BidAction, BiddingMode, BiddingOutcome, BiddingSession};
    pub use super::card::{DouDizhuCard, DouDizhuRank, standard_deck};
    pub use super::comparator::DouDizhuComparator;
    pub use super::effects::{
        AwardScoreCmd, BombMultiplierListener, ClearTableCmd, RoundEndListener, SettlementListener,
        SpringListener,
    };
    pub use super::evaluator::DouDizhuEvaluator;
    pub use super::pattern::{HandPattern, PatternRecognizer, PatternType};
    pub use super::scoring::{GameOutcome, ScoreBoard, Side};
    pub use super::setup::{DouDizhuConfig, DouDizhuSetup};
    pub use super::validator::DouDizhuValidator;
}
