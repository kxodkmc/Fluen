//! 斗地主预设端到端集成测试。

use crate::core::{Deck, Hand};
use crate::engine::{Action, GameEngine, GameError, GameState, Phase};
use crate::presets::dou_dizhu::bidding::{BidAction, BiddingMode, BiddingSession};
use crate::presets::dou_dizhu::card::{DouDizhuCard, DouDizhuRank};
use crate::presets::dou_dizhu::effects::{
    BombMultiplierListener, RoundEndListener, SettlementListener, SpringListener,
};
use crate::presets::dou_dizhu::scoring::ScoreBoard;
use crate::presets::dou_dizhu::setup::{DouDizhuConfig, DouDizhuSetup};
use crate::presets::dou_dizhu::validator::DouDizhuValidator;
use crate::rule::PlayerStatesView;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// 构造一张斗地主牌的便捷函数。
fn c(id: u32, rank: DouDizhuRank) -> DouDizhuCard {
    DouDizhuCard::new(id, rank)
}

/// 构造最小可用状态：指定玩家手牌、桌面牌与当前轮次玩家。
fn make_state(
    hands: HashMap<u32, Vec<DouDizhuCard>>,
    table: Vec<DouDizhuCard>,
    current_player: u32,
) -> GameState<DouDizhuCard> {
    let mut players_hands: HashMap<u32, Hand<DouDizhuCard>> = HashMap::new();
    let mut player_states = PlayerStatesView::default();
    for (pid, cards) in hands {
        let mut h = Hand::new();
        for card in cards {
            h.add(card);
        }
        player_states.hands_count.insert(pid, h.len());
        players_hands.insert(pid, h);
    }
    GameState {
        phase: Phase::Playing,
        current_player,
        deck: Deck::new(),
        players_hands,
        table_cards: table,
        played_cards: Vec::new(),
        history: VecDeque::new(),
        player_states,
    }
}

/// Test 1: 地主出完手牌后触发结算，地主胜计分正确。
#[test]
fn minimal_landlord_win_settles() {
    let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(3)));
    let mut hands = HashMap::new();
    hands.insert(
        1u32,
        vec![c(0, DouDizhuRank::Three), c(1, DouDizhuRank::Three)],
    );
    hands.insert(2u32, vec![c(2, DouDizhuRank::Four)]);
    hands.insert(3u32, vec![c(3, DouDizhuRank::Five)]);
    let state = make_state(hands, vec![], 1);

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    engine.register_listener(SettlementListener::new(scoreboard.clone(), 1, [2, 3]));

    // 地主出对子 → 手牌为 0 → 终局结算
    let result = engine.transition(Action::Play(1, vec![0, 1]));
    assert!(result.is_ok());

    // final = base(3) × multiplier(1) = 3；地主 +6，农民各 -3
    assert_eq!(engine.state.player_states.scores.get(&1), Some(&6));
    assert_eq!(engine.state.player_states.scores.get(&2), Some(&-3));
    assert_eq!(engine.state.player_states.scores.get(&3), Some(&-3));
    let sb = scoreboard.lock().expect("scoreboard lock");
    assert_eq!(sb.final_score(), 3);
}

/// Test 2: 农民出完手牌后触发结算，农民胜计分正确。
#[test]
fn minimal_farmer_win_settles() {
    let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(3)));
    let mut hands = HashMap::new();
    hands.insert(1u32, vec![c(0, DouDizhuRank::Three)]);
    hands.insert(2u32, vec![c(1, DouDizhuRank::Four)]);
    hands.insert(3u32, vec![c(2, DouDizhuRank::Five)]);
    let state = make_state(hands, vec![], 2);

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    engine.register_listener(SettlementListener::new(scoreboard.clone(), 1, [2, 3]));

    // 农民 2 出单张 → 手牌为 0 → 终局结算
    let result = engine.transition(Action::Play(2, vec![1]));
    assert!(result.is_ok());

    // final = base(3) × multiplier(1) = 3；地主 -6，农民各 +3
    assert_eq!(engine.state.player_states.scores.get(&1), Some(&-6));
    assert_eq!(engine.state.player_states.scores.get(&2), Some(&3));
    assert_eq!(engine.state.player_states.scores.get(&3), Some(&3));
}

/// Test 3: 连续 2 次 Pass 后 RoundEndListener 清空桌面，恢复自由出牌。
#[test]
fn round_end_clears_table_after_two_passes() {
    let mut hands = HashMap::new();
    hands.insert(
        1u32,
        vec![c(0, DouDizhuRank::Three), c(1, DouDizhuRank::Four)],
    );
    hands.insert(2u32, vec![c(2, DouDizhuRank::Five)]);
    hands.insert(3u32, vec![c(3, DouDizhuRank::Six)]);
    let state = make_state(hands, vec![], 1);

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    engine.register_listener(RoundEndListener::default());

    // 玩家 1 出单张 Three
    engine.state.current_player = 1;
    assert!(engine.transition(Action::Play(1, vec![0])).is_ok());
    assert_eq!(engine.state.table_cards.len(), 1);

    // 非当前轮次出牌应被拒绝
    engine.state.current_player = 2;
    assert_eq!(
        engine.transition(Action::Play(1, vec![1])),
        Err(GameError::InvalidAction)
    );

    // 玩家 2 Pass
    engine.state.current_player = 2;
    assert!(engine.transition(Action::Pass(2)).is_ok());
    assert_eq!(engine.state.table_cards.len(), 1);

    // 玩家 3 Pass → 连续 2 次 Pass → 清桌
    engine.state.current_player = 3;
    assert!(engine.transition(Action::Pass(3)).is_ok());
    assert!(engine.state.table_cards.is_empty());

    // 玩家 1 自由出牌 Four
    engine.state.current_player = 1;
    assert!(engine.transition(Action::Play(1, vec![1])).is_ok());
    assert_eq!(engine.state.table_cards.len(), 1);
}

/// Test 4: 炸弹触发 BombMultiplierListener 翻倍。
#[test]
fn bomb_doubles_scoreboard() {
    let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(1)));
    let mut hands = HashMap::new();
    hands.insert(
        1u32,
        vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
        ],
    );
    let state = make_state(hands, vec![], 1);

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    engine.register_listener(BombMultiplierListener::new(scoreboard.clone(), 2, 2));

    assert!(engine.transition(Action::Play(1, vec![0, 1, 2, 3])).is_ok());

    let sb = scoreboard.lock().expect("scoreboard lock");
    assert_eq!(sb.multiplier(), 2);
}

/// Test 5: 叫地主会话完整流程：1 叫 1 分，2 叫 2 分，3 不叫 → 玩家 2 成为地主。
#[test]
fn bidding_session_full_flow() {
    let mut session = BiddingSession::new(
        BiddingMode::Classic { max_score: 3 },
        [1, 2, 3],
        vec![100, 101, 102],
    );
    assert!(session.submit(1, BidAction::Bid(1)).is_ok());
    assert!(session.submit(2, BidAction::Bid(2)).is_ok());
    assert!(session.submit(3, BidAction::Pass).is_ok());
    assert!(session.is_finished());
    let outcome = session.outcome().expect("bidding finished with winner");
    assert_eq!(outcome.landlord, 2);
    assert_eq!(outcome.bid_score, 2);
    assert_eq!(outcome.bottom_card_ids, vec![100, 101, 102]);
}

/// Test 6: prelude 导出可达性编译检查。
#[test]
fn prelude_reachable() {
    use crate::presets::dou_dizhu::prelude::*;
    let _ = DouDizhuConfig::default();
    let _ = DouDizhuValidator::new();
    let _ = ScoreBoard::new(1);
    let _ = DouDizhuCard::new(0, DouDizhuRank::Three);
}

/// Test 7: 反春天翻倍 — 地主胜且两农民未出牌时倍数 ×2。
#[test]
fn anti_spring_doubles_on_landlord_win() {
    let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(1)));
    let mut hands = HashMap::new();
    hands.insert(
        1u32,
        vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Five)],
    );
    hands.insert(2u32, vec![c(2, DouDizhuRank::Three)]);
    hands.insert(3u32, vec![c(3, DouDizhuRank::Three)]);
    let state = make_state(hands, vec![], 1);

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    // SpringListener 须在 SettlementListener 之前注册
    engine.register_listener(SpringListener::new(
        scoreboard.clone(),
        1,
        [2, 3],
        true,
        true,
        2,
    ));
    engine.register_listener(SettlementListener::new(scoreboard.clone(), 1, [2, 3]));

    // 地主出对子 → 手牌为 0 → 终局
    assert!(engine.transition(Action::Play(1, vec![0, 1])).is_ok());

    // 反春天：两农民均未出牌 → multiplier ×2 = 2
    // final = base(1) × multiplier(2) = 2；地主 +4，农民各 -2
    {
        let sb = scoreboard.lock().expect("scoreboard lock");
        assert_eq!(sb.multiplier(), 2);
    }
    assert_eq!(engine.state.player_states.scores.get(&1), Some(&4));
    assert_eq!(engine.state.player_states.scores.get(&2), Some(&-2));
    assert_eq!(engine.state.player_states.scores.get(&3), Some(&-2));
}

/// Test 8: 完整发牌 + 叫地主 + 分配底牌流程，验证状态一致性。
#[test]
fn full_deal_and_assign_bottom_flow() {
    let config = DouDizhuConfig::default();
    let mut rng = StdRng::seed_from_u64(42);
    let (mut session, mut state) = DouDizhuSetup::new_game(&config, [1, 2, 3], &mut rng);

    // 各玩家 17 张，底牌 3 张存于 deck，phase=Bidding
    assert_eq!(state.players_hands.get(&1).expect("player 1").len(), 17);
    assert_eq!(state.players_hands.get(&2).expect("player 2").len(), 17);
    assert_eq!(state.players_hands.get(&3).expect("player 3").len(), 17);
    assert_eq!(state.deck.len(), 3);
    assert_eq!(state.phase, Phase::Bidding);

    // 叫地主：玩家 1 叫 3 分（max）直接定地主
    session.submit(1, BidAction::Bid(3)).expect("bid ok");
    let outcome = session.outcome().expect("landlord determined");
    let landlord = outcome.landlord;
    assert_eq!(landlord, 1);
    assert_eq!(outcome.bid_score, 3);

    // 分配底牌
    DouDizhuSetup::assign_bottom(&mut state, landlord);
    assert_eq!(
        state.players_hands.get(&landlord).expect("landlord").len(),
        20
    );
    assert_eq!(state.deck.len(), 0);
    assert_eq!(state.phase, Phase::Playing);
    assert_eq!(state.current_player, landlord);
}
