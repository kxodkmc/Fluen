//! Engine 层 — 阶段状态机、CQRS 命令总线与事件分发。
//!
//! 设计要点：
//! - [`GameState`] 持有完整状态，[`GameState::build_context`] 构建只读 [`GameContext`]
//! - 副作用通过 [`GameCommand`] Trait 封装，由 [`EventListener`] 在事件钩子中返回，
//!   引擎统一在动作成功后执行，失败则丢弃命令，避免半提交
//! - 所有动态分发 Trait Object 加 `Send + Sync`，保证引擎可跨线程传递
//! - 历史使用 `VecDeque` 并限制容量上限，防止内存与性能爆炸

use crate::core::CoreError;
use crate::entity::CardEntity;
use crate::rule::{ActionValidator, GameContext, PlayerStatesView};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;

/// 历史滑动窗口容量上限
pub const HISTORY_LIMIT: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Bidding,
    Playing,
    Responding,
    Settling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// (player_id, card_ids)
    Play(u32, Vec<u32>),
    /// (player_id)
    Pass(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameError {
    InvalidAction,
    CoreError(CoreError),
    StateError(String),
}

impl From<CoreError> for GameError {
    fn from(e: CoreError) -> Self {
        GameError::CoreError(e)
    }
}

/// 完整的游戏状态机
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState<C: CardEntity> {
    pub phase: Phase,
    pub current_player: u32,
    pub deck: crate::core::Deck<C>,
    pub players_hands: HashMap<u32, crate::core::Hand<C>>,
    pub table_cards: Vec<C>,
    pub played_cards: Vec<C>,
    pub history: VecDeque<Action>,
    pub player_states: PlayerStatesView,
}

impl<C: CardEntity> GameState<C> {
    /// 构建不可变上下文，提供最近历史切片。
    ///
    /// 注意：取 `&mut self` 仅为调用 `VecDeque::make_contiguous` 以获得连续切片，
    /// 逻辑上不改变历史序列。
    pub fn build_context(&mut self) -> GameContext<'_, C> {
        let history: &[Action] = self.history.make_contiguous();
        let current_hand: &[C] = match self.players_hands.get(&self.current_player) {
            Some(h) => h.iter(),
            None => &[],
        };
        GameContext {
            current_player: self.current_player,
            phase: self.phase,
            history,
            table_cards: &self.table_cards,
            current_hand,
            players_state: &self.player_states,
            played_cards: &self.played_cards,
        }
    }
}

/// 命令系统 Trait：下游可自由实现此 Trait 生成自定义副作用
pub trait GameCommand<C: CardEntity>: Debug {
    fn execute(&self, state: &mut GameState<C>);
}

/// 引擎事件。
///
/// 注：变体均不引用卡牌类型 `C`，故本枚举为非泛型，避免 `PhantomData` 噪声。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    BeforeAction(Action),
    AfterAction(Action),
    PhaseChange(Phase),
}

/// 钩子 Trait：监听事件，返回命令列表
pub trait EventListener<C: CardEntity> {
    fn on_event(&mut self, event: &GameEvent) -> Vec<Box<dyn GameCommand<C> + Send + Sync>>;
}

pub struct GameEngine<C: CardEntity, V: ActionValidator<C>> {
    pub validator: V,
    pub state: GameState<C>,
    listeners: Vec<Box<dyn EventListener<C> + Send + Sync>>,
}

impl<C: CardEntity, V: ActionValidator<C>> GameEngine<C, V> {
    pub fn new(validator: V, initial_state: GameState<C>) -> Self {
        Self {
            validator,
            state: initial_state,
            listeners: Vec::new(),
        }
    }

    pub fn register_listener<L: EventListener<C> + Send + Sync + 'static>(&mut self, listener: L) {
        self.listeners.push(Box::new(listener));
    }

    /// 状态流转入口
    pub fn transition(&mut self, action: Action) -> Result<(), GameError> {
        // 1. 构建不可变 Context 并校验；使用块作用域收紧对 self.state 的借用，
        //    使得校验通过后 NLL 立即释放，后续可继续 &mut self。
        let valid = {
            let ctx = self.state.build_context();
            self.validator.validate(&ctx, &action)
        };
        if !valid {
            return Err(GameError::InvalidAction);
        }

        // 2. 生成前置事件并收集命令
        let event = GameEvent::BeforeAction(action.clone());
        let mut commands = self.emit(&event);

        // 3. 执行动作，修改 State（若失败，commands 直接丢弃，无半提交风险）
        self.apply_action(&action)?;

        // 4. 记录历史并维持容量上限
        self.state.history.push_back(action.clone());
        while self.state.history.len() > HISTORY_LIMIT {
            self.state.history.pop_front();
        }

        // 5. 生成后置事件
        let event = GameEvent::AfterAction(action);
        commands.extend(self.emit(&event));

        // 6. 统一执行副作用命令
        self.execute_commands(commands);
        Ok(())
    }

    fn emit(&mut self, event: &GameEvent) -> Vec<Box<dyn GameCommand<C> + Send + Sync>> {
        let mut cmds = Vec::new();
        for listener in &mut self.listeners {
            cmds.extend(listener.on_event(event));
        }
        cmds
    }

    fn apply_action(&mut self, action: &Action) -> Result<(), GameError> {
        match action {
            Action::Play(pid, card_ids) => {
                let hand = self
                    .state
                    .players_hands
                    .get_mut(pid)
                    .ok_or_else(|| GameError::StateError(format!("player {pid} not found")))?;
                let mut played = Vec::with_capacity(card_ids.len());
                for id in card_ids {
                    played.push(hand.remove(*id)?);
                }
                self.state.table_cards.extend(played.clone());
                self.state.played_cards.extend(played);
                // 更新视野数据
                let count = self
                    .state
                    .player_states
                    .hands_count
                    .entry(*pid)
                    .or_insert(0);
                *count = count.saturating_sub(card_ids.len());
            }
            Action::Pass(_pid) => {
                // 跳过：仅记录历史，不修改手牌
            }
        }
        Ok(())
    }

    fn execute_commands(&mut self, commands: Vec<Box<dyn GameCommand<C> + Send + Sync>>) {
        for cmd in commands {
            cmd.execute(&mut self.state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CoreError, Deck, Hand};
    use crate::entity::{StandardCard, StandardSuit};
    use crate::rule::{ActionValidator, GameContext, PlayerStatesView};
    use std::collections::{HashMap, VecDeque};

    /// 永远返回 `true` 的校验器，用于测试中绕过规则校验。
    struct AlwaysValidator;
    impl<C: CardEntity> ActionValidator<C> for AlwaysValidator {
        fn validate(&self, _ctx: &GameContext<'_, C>, _action: &Action) -> bool {
            true
        }
    }

    /// 永远返回 `false` 的校验器，用于测试非法动作分支。
    struct NeverValidator;
    impl<C: CardEntity> ActionValidator<C> for NeverValidator {
        fn validate(&self, _ctx: &GameContext<'_, C>, _action: &Action) -> bool {
            false
        }
    }

    /// 构造一个最小可用状态：玩家 1 持有一张 id=0 的牌，并同步 hands_count。
    fn make_state_with_one_card(deck_cards: Vec<StandardCard>) -> GameState<StandardCard> {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(0, StandardSuit::Spade, 3));
        let mut state = GameState {
            phase: Phase::Playing,
            current_player: 1,
            deck: Deck::from_vec(deck_cards),
            players_hands: HashMap::from([(1u32, hand)]),
            table_cards: Vec::new(),
            played_cards: Vec::new(),
            history: VecDeque::new(),
            player_states: PlayerStatesView::default(),
        };
        state.player_states.hands_count.insert(1, 1);
        state
    }

    /// 合法 Play 动作后，手牌、桌面、历史与视野应同步更新。
    #[test]
    fn valid_play_moves_cards() {
        let deck_cards = vec![
            StandardCard::new(10, StandardSuit::Spade, 3),
            StandardCard::new(11, StandardSuit::Spade, 4),
        ];
        let state = make_state_with_one_card(deck_cards);
        let mut engine = GameEngine::new(AlwaysValidator, state);

        let result = engine.transition(Action::Play(1, vec![0]));
        assert!(result.is_ok());

        let hand = engine.state.players_hands.get(&1).expect("player 1 exists");
        assert!(hand.is_empty());
        assert_eq!(engine.state.table_cards.len(), 1);
        assert_eq!(engine.state.table_cards[0].id(), 0);
        assert_eq!(engine.state.played_cards.len(), 1);
        assert_eq!(engine.state.player_states.hands_count.get(&1), Some(&0));
        assert_eq!(engine.state.history.len(), 1);
    }

    /// 校验失败时状态不应改变，返回 `InvalidAction`。
    #[test]
    fn invalid_action_rejected_state_unchanged() {
        let state = make_state_with_one_card(vec![]);
        let mut engine = GameEngine::new(NeverValidator, state);

        let result = engine.transition(Action::Play(1, vec![0]));
        assert_eq!(result, Err(GameError::InvalidAction));

        let hand = engine.state.players_hands.get(&1).expect("player 1 exists");
        assert_eq!(hand.len(), 1);
        assert!(engine.state.table_cards.is_empty());
        assert!(engine.state.history.is_empty());
    }

    /// `DrawTwoCmd`：从牌堆摸两张牌加入当前玩家手牌。
    #[derive(Debug)]
    struct DrawTwoCmd;
    impl<C: CardEntity> GameCommand<C> for DrawTwoCmd {
        fn execute(&self, state: &mut GameState<C>) {
            for _ in 0..2 {
                if let Some(card) = state.deck.deal() {
                    state
                        .players_hands
                        .entry(state.current_player)
                        .or_default()
                        .add(card);
                }
            }
        }
    }

    /// `DrawListener`：在 `AfterAction(Play)` 事件触发时返回 `DrawTwoCmd`。
    struct DrawListener;
    impl<C: CardEntity> EventListener<C> for DrawListener {
        fn on_event(&mut self, event: &GameEvent) -> Vec<Box<dyn GameCommand<C> + Send + Sync>> {
            if let GameEvent::AfterAction(Action::Play(..)) = event {
                vec![Box::new(DrawTwoCmd)]
            } else {
                vec![]
            }
        }
    }

    /// 监听器在 AfterAction 返回的命令应在动作成功后执行，摸两张牌。
    #[test]
    fn event_listener_side_effect() {
        let deck_cards: Vec<StandardCard> = (100u32..105)
            .map(|i| StandardCard::new(i, StandardSuit::Spade, 3))
            .collect();
        let state = make_state_with_one_card(deck_cards);
        let mut engine = GameEngine::new(AlwaysValidator, state);
        engine.register_listener(DrawListener);

        let result = engine.transition(Action::Play(1, vec![0]));
        assert!(result.is_ok());

        // 原手牌 1 张被打出，随后摸 2 张 → 手牌长度 2；牌堆 5 - 2 = 3。
        let hand = engine.state.players_hands.get(&1).expect("player 1 exists");
        assert_eq!(hand.len(), 2);
        assert_eq!(engine.state.deck.len(), 3);
    }

    /// 历史超过容量上限时应丢弃最旧记录，保持长度 == `HISTORY_LIMIT`。
    #[test]
    fn history_capacity_limit() {
        let state: GameState<StandardCard> = GameState {
            phase: Phase::Playing,
            current_player: 1,
            deck: Deck::new(),
            players_hands: HashMap::from([(1u32, Hand::new())]),
            table_cards: Vec::new(),
            played_cards: Vec::new(),
            history: VecDeque::new(),
            player_states: PlayerStatesView::default(),
        };
        let mut engine = GameEngine::new(AlwaysValidator, state);

        for _ in 0..2000 {
            engine.transition(Action::Pass(1)).expect("pass always ok");
        }
        assert_eq!(engine.state.history.len(), HISTORY_LIMIT);
    }

    /// `CoreError` 可通过 `From` 转换为 `GameError`。
    #[test]
    fn from_core_error_conversion() {
        let ge: GameError = CoreError::CardNotFound.into();
        assert_eq!(ge, GameError::CoreError(CoreError::CardNotFound));
    }

    /// Play 不存在的玩家应返回 `StateError`。
    #[test]
    fn play_missing_player_returns_state_error() {
        let state: GameState<StandardCard> = GameState {
            phase: Phase::Playing,
            current_player: 1,
            deck: Deck::new(),
            players_hands: HashMap::new(),
            table_cards: Vec::new(),
            played_cards: Vec::new(),
            history: VecDeque::new(),
            player_states: PlayerStatesView::default(),
        };
        let mut engine = GameEngine::new(AlwaysValidator, state);

        let result = engine.transition(Action::Play(99, vec![0]));
        assert!(matches!(result, Err(GameError::StateError(_))));
    }

    /// Play 玩家手中不存在的牌 id 应返回 `CoreError::CardNotFound`。
    #[test]
    fn play_missing_card_returns_core_error() {
        let state = make_state_with_one_card(vec![]);
        let mut engine = GameEngine::new(AlwaysValidator, state);

        let result = engine.transition(Action::Play(1, vec![999]));
        assert_eq!(result, Err(GameError::CoreError(CoreError::CardNotFound)));
    }

    /// 编译期断言：`GameEngine<C, V>` 在 `C`/`V` 满足 `Send + Sync` 时同样满足。
    #[allow(dead_code)]
    fn _assert_send_sync<C: CardEntity + Send + Sync, V: ActionValidator<C> + Send + Sync>() {
        fn _assert<T: Send + Sync>() {}
        _assert::<GameEngine<C, V>>();
    }
}
