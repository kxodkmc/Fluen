//! Boardgame 预导入：常用类型一键可用。
pub use crate::core::{CoreError, Deck, Hand};
pub use crate::engine::{
    Action, EventListener, GameCommand, GameEngine, GameError, GameEvent, GameState, HISTORY_LIMIT,
    Phase,
};
pub use crate::entity::{CardEntity, StandardCard, StandardSuit};
pub use crate::rule::{
    ActionValidator, GameContext, HandEvaluator, HandValue, PlayerStatesView, ValueComparator,
};
pub use crate::test_kit::{Replayer, deterministic_rng};
