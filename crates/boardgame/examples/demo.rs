//! boardgame 框架完整使用示例：
//! 定义实体 → 构建状态 → 实现校验器 → 注册副作用监听 → 驱动 → 回放断言。

use boardgame::prelude::*;
use boardgame::rule::GameContext;
use std::collections::{HashMap, VecDeque};

/// 简单校验器：打出牌时校验轮次与持牌。
struct SimpleValidator;
impl ActionValidator<StandardCard> for SimpleValidator {
    fn validate(&self, ctx: &GameContext<'_, StandardCard>, action: &Action) -> bool {
        match action {
            Action::Pass(_) => true,
            Action::Play(pid, card_ids) => {
                *pid == ctx.current_player
                    && card_ids
                        .iter()
                        .all(|id| ctx.current_hand.iter().any(|c| c.id() == *id))
            }
        }
    }
}

/// 副作用命令：给指定玩家加分。
#[derive(Debug)]
struct AwardScoreCmd {
    player: u32,
    delta: i32,
}
impl GameCommand<StandardCard> for AwardScoreCmd {
    fn execute(&self, state: &mut GameState<StandardCard>) {
        *state.player_states.scores.entry(self.player).or_insert(0) += self.delta;
    }
}

/// 事件监听器：打出牌后给该玩家 +10 分（演示开闭原则扩展）。
struct ScoreListener;
impl EventListener<StandardCard> for ScoreListener {
    fn on_event(
        &mut self,
        event: &GameEvent,
    ) -> Vec<Box<dyn GameCommand<StandardCard> + Send + Sync>> {
        if let GameEvent::AfterAction(Action::Play(pid, cards)) = event
            && !cards.is_empty()
        {
            return vec![Box::new(AwardScoreCmd {
                player: *pid,
                delta: 10,
            })];
        }
        vec![]
    }
}

fn build_initial_state() -> GameState<StandardCard> {
    let mut hands = HashMap::new();
    let mut h1 = Hand::new();
    h1.add(StandardCard::new(0, StandardSuit::Spade, 3));
    h1.add(StandardCard::new(1, StandardSuit::Heart, 5));
    let mut h2 = Hand::new();
    h2.add(StandardCard::new(2, StandardSuit::Club, 7));
    h2.add(StandardCard::new(3, StandardSuit::Diamond, 9));
    hands.insert(1u32, h1);
    hands.insert(2u32, h2);

    let mut player_states = boardgame::rule::PlayerStatesView::default();
    player_states.hands_count.insert(1, 2);
    player_states.hands_count.insert(2, 2);

    GameState {
        phase: Phase::Playing,
        current_player: 1,
        deck: Deck::new(),
        players_hands: hands,
        table_cards: Vec::new(),
        played_cards: Vec::new(),
        history: VecDeque::new(),
        player_states,
    }
}

fn main() {
    // === 1. 副作用扩展演示（开闭原则） ===
    let initial = build_initial_state();
    let snapshot_for_replay = initial.clone();

    let mut engine = GameEngine::new(SimpleValidator, initial);
    engine.register_listener(ScoreListener);

    engine.transition(Action::Play(1, vec![0])).unwrap();
    println!(
        "[副作用] 玩家1打出牌后得分: {:?}",
        engine.state.player_states.scores.get(&1)
    );
    println!("[状态] 桌面牌数: {}", engine.state.table_cards.len());

    // === 2. 回放断言演示（确定性） ===
    // 回放器不携带监听器，故与「无监听器」引擎对比。
    let actions = vec![Action::Play(1, vec![0])];
    let replayer = Replayer::<StandardCard, SimpleValidator>::new(
        snapshot_for_replay.clone(),
        actions.clone(),
    );
    let replayed = replayer.replay(SimpleValidator).unwrap();

    let mut engine_b = GameEngine::new(SimpleValidator, snapshot_for_replay);
    for a in &actions {
        engine_b.transition(a.clone()).unwrap();
    }
    assert_eq!(engine_b.state, replayed, "游戏状态快照不一致");
    println!("[回放] 状态快照一致 ✓");

    // === 3. 确定性洗牌演示 ===
    let mut deck = Deck::from_vec(StandardCard::standard_deck());
    let mut rng = boardgame::test_kit::deterministic_rng(42);
    deck.shuffle(&mut rng);
    println!(
        "[洗牌] 洗后牌堆顶 id: {:?}",
        deck.iter().last().map(|c| c.id())
    );

    println!("boardgame 示例运行完成！");
}
