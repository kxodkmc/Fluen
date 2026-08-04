//! Test kit 层 — 确定性回放、状态快照断言与确定性 RNG。
//!
//! 设计要点：
//! - [`Replayer`] 从给定初始状态依次应用动作序列，返回最终状态，用于回归测试
//! - [`assert_state_snapshot!`] 宏对比引擎状态与期望状态
//! - [`deterministic_rng`] 基于固定种子构造 RNG，保证洗牌等过程可精确重现

use crate::engine::{Action, GameEngine, GameError, GameState};
use crate::entity::CardEntity;
use crate::rule::ActionValidator;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::marker::PhantomData;

/// 构造一个固定种子的确定性 RNG，用于洗牌等需要可重现的场景。
pub fn deterministic_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

/// 确定性回放器：从给定初始状态依次应用动作序列，返回最终状态。
pub struct Replayer<C: CardEntity, V: ActionValidator<C>> {
    pub initial_state: GameState<C>,
    pub actions: Vec<Action>,
    _marker: PhantomData<V>,
}

impl<C: CardEntity, V: ActionValidator<C>> Replayer<C, V> {
    pub fn new(initial_state: GameState<C>, actions: Vec<Action>) -> Self {
        Self {
            initial_state,
            actions,
            _marker: PhantomData,
        }
    }

    /// 用传入的校验器回放动作序列，返回深拷贝的最终状态。
    pub fn replay(&self, validator: V) -> Result<GameState<C>, GameError> {
        let mut engine = GameEngine::<C, V>::new(validator, self.initial_state.clone());
        for action in &self.actions {
            engine.transition(action.clone())?;
        }
        Ok(engine.state)
    }
}

/// 状态快照断言宏：对比引擎当前状态与期望状态。
#[macro_export]
macro_rules! assert_state_snapshot {
    ($engine:expr, $expected:expr) => {
        assert_eq!($engine.state, $expected, "游戏状态快照不一致");
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Deck, Hand};
    use crate::engine::{Action, GameEngine, GameState, Phase};
    use crate::entity::{CardEntity, StandardCard, StandardSuit};
    use crate::rule::{ActionValidator, GameContext, PlayerStatesView};
    use std::collections::{HashMap, VecDeque};

    /// 始终放行的校验器
    struct AlwaysValidator;
    impl<C: CardEntity> ActionValidator<C> for AlwaysValidator {
        fn validate(&self, _ctx: &GameContext<'_, C>, _action: &Action) -> bool {
            true
        }
    }

    fn sample_state() -> GameState<StandardCard> {
        let mut hand1 = Hand::new();
        hand1.add(StandardCard::new(0, StandardSuit::Spade, 3));
        hand1.add(StandardCard::new(1, StandardSuit::Heart, 5));
        let mut hands = HashMap::new();
        hands.insert(1u32, hand1);
        let mut ps = PlayerStatesView::default();
        ps.hands_count.insert(1, 2);
        GameState {
            phase: Phase::Playing,
            current_player: 1,
            deck: Deck::new(),
            players_hands: hands,
            table_cards: Vec::new(),
            played_cards: Vec::new(),
            history: VecDeque::new(),
            player_states: ps,
        }
    }

    /// 相同输入两次回放，结果完全一致。
    #[test]
    fn replayer_is_deterministic() {
        let state = sample_state();
        let actions = vec![Action::Play(1, vec![0]), Action::Pass(1)];
        let replayer = Replayer::<StandardCard, AlwaysValidator>::new(state, actions);
        let final_a = replayer.replay(AlwaysValidator).unwrap();
        let final_b = replayer.replay(AlwaysValidator).unwrap();
        assert_eq!(final_a, final_b);
    }

    /// 回放正确应用动作序列。
    #[test]
    fn replayer_applies_actions() {
        let state = sample_state();
        let actions = vec![Action::Play(1, vec![0])];
        let replayer = Replayer::<StandardCard, AlwaysValidator>::new(state, actions);
        let final_state = replayer.replay(AlwaysValidator).unwrap();
        assert_eq!(final_state.players_hands.get(&1).unwrap().len(), 1);
        assert_eq!(final_state.table_cards.len(), 1);
        assert_eq!(final_state.history.len(), 1);
    }

    /// 回放中遇到非法动作返回错误。
    #[test]
    fn replayer_propagates_errors() {
        struct NeverValidator;
        impl<C: CardEntity> ActionValidator<C> for NeverValidator {
            fn validate(&self, _ctx: &GameContext<'_, C>, _action: &Action) -> bool {
                false
            }
        }
        let state = sample_state();
        let actions = vec![Action::Play(1, vec![0])];
        let replayer = Replayer::<StandardCard, NeverValidator>::new(state, actions);
        let result = replayer.replay(NeverValidator);
        assert!(matches!(result, Err(GameError::InvalidAction)));
    }

    /// 确定性 RNG 同种子同结果。
    #[test]
    fn deterministic_rng_is_reproducible() {
        let mut rng_a = deterministic_rng(123);
        let mut rng_b = deterministic_rng(123);
        let mut deck_a = Deck::from_vec(StandardCard::standard_deck());
        let mut deck_b = Deck::from_vec(StandardCard::standard_deck());
        deck_a.shuffle(&mut rng_a);
        deck_b.shuffle(&mut rng_b);
        assert_eq!(deck_a, deck_b);
    }

    /// 快照宏在状态一致时通过。
    #[test]
    fn snapshot_macro_passes_when_equal() {
        let mut engine = GameEngine::new(AlwaysValidator, sample_state());
        engine.transition(Action::Pass(1)).unwrap();
        let expected = engine.state.clone();
        assert_state_snapshot!(engine, expected);
    }

    /// 快照宏在状态不一致时 panic。
    #[test]
    #[should_panic(expected = "游戏状态快照不一致")]
    fn snapshot_macro_panics_when_unequal() {
        let mut engine = GameEngine::new(AlwaysValidator, sample_state());
        engine.transition(Action::Pass(1)).unwrap();
        let different = sample_state();
        assert_state_snapshot!(engine, different);
    }
}
