//! 斗地主预制副作用（GameCommand 与 EventListener）。
//!
//! 提供开箱即用的：
//! - [`ClearTableCmd`]：清空桌面牌（新一轮自由出牌）
//! - [`AwardScoreCmd`]：给指定玩家加分
//! - [`RoundEndListener`]：连续 2 次 Pass 后清桌
//! - [`BombMultiplierListener`]：炸弹/王炸触发 ScoreBoard 翻倍
//! - [`SpringListener`]：终局春天/反春天检测
//! - [`SettlementListener`]：终局结算写入 player_states.scores
//!
//! 所有 Listener/Command 满足 `Send + Sync`，可在多线程引擎中使用。
//!
//! # 监听器注册顺序
//! [`SpringListener`] 须在 [`SettlementListener`] 之前注册，以确保春天/反春天
//! 倍数在结算前已写入 `ScoreBoard`。引擎按注册顺序收集 `AfterAction` 命令并依次执行。

use crate::engine::{Action, EventListener, GameCommand, GameEvent, GameState};
use crate::presets::dou_dizhu::card::DouDizhuCard;
use crate::presets::dou_dizhu::pattern::{PatternRecognizer, PatternType};
use crate::presets::dou_dizhu::scoring::{ScoreBoard, Side};
use std::sync::{Arc, Mutex};

/// 清空桌面牌命令（新一轮自由出牌）。
#[derive(Debug, Clone, Copy)]
pub struct ClearTableCmd;

impl GameCommand<DouDizhuCard> for ClearTableCmd {
    fn execute(&self, state: &mut GameState<DouDizhuCard>) {
        state.table_cards.clear();
    }
}

/// 给指定玩家加分的命令。
#[derive(Debug, Clone, Copy)]
pub struct AwardScoreCmd {
    pub player: u32,
    pub delta: i32,
}

impl GameCommand<DouDizhuCard> for AwardScoreCmd {
    fn execute(&self, state: &mut GameState<DouDizhuCard>) {
        *state.player_states.scores.entry(self.player).or_insert(0) += self.delta;
    }
}

/// 检测连续 2 次 Pass 后清桌的监听器。
///
/// 触发条件：`AfterAction` 中检测到连续两个 `Pass` 动作。
/// 监听器内部维护连续 Pass 计数器，遇到 `Play` 动作时重置为 0。
#[derive(Debug, Default)]
pub struct RoundEndListener {
    consecutive_passes: u32,
}

impl EventListener<DouDizhuCard> for RoundEndListener {
    fn on_event(
        &mut self,
        event: &GameEvent,
    ) -> Vec<Box<dyn GameCommand<DouDizhuCard> + Send + Sync>> {
        match event {
            GameEvent::AfterAction(Action::Pass(_)) => {
                self.consecutive_passes += 1;
                if self.consecutive_passes >= 2 {
                    self.consecutive_passes = 0;
                    return vec![Box::new(ClearTableCmd)];
                }
                vec![]
            }
            GameEvent::AfterAction(Action::Play(..)) => {
                self.consecutive_passes = 0;
                vec![]
            }
            _ => vec![],
        }
    }
}

/// 炸弹/王炸翻倍监听器：持有共享 `ScoreBoard`。
///
/// 出牌为炸弹或王炸时调用 `scoreboard.multiply(...)`。
pub struct BombMultiplierListener {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    bomb_multiplier: u32,
    rocket_multiplier: u32,
}

impl BombMultiplierListener {
    pub fn new(
        scoreboard: Arc<Mutex<ScoreBoard>>,
        bomb_multiplier: u32,
        rocket_multiplier: u32,
    ) -> Self {
        Self {
            scoreboard,
            bomb_multiplier,
            rocket_multiplier,
        }
    }
}

impl EventListener<DouDizhuCard> for BombMultiplierListener {
    fn on_event(
        &mut self,
        event: &GameEvent,
    ) -> Vec<Box<dyn GameCommand<DouDizhuCard> + Send + Sync>> {
        if let GameEvent::AfterAction(Action::Play(_, card_ids)) = event {
            // 事件仅提供 card_ids，实际牌实体须在命令执行时从 state 读取
            return vec![Box::new(MultiplyIfBombCmd {
                scoreboard: self.scoreboard.clone(),
                card_ids: card_ids.clone(),
                bomb_multiplier: self.bomb_multiplier,
                rocket_multiplier: self.rocket_multiplier,
            })];
        }
        vec![]
    }
}

/// 延迟翻倍命令：在 execute 时访问 state 取牌识别。
#[derive(Debug)]
struct MultiplyIfBombCmd {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    card_ids: Vec<u32>,
    bomb_multiplier: u32,
    rocket_multiplier: u32,
}

impl GameCommand<DouDizhuCard> for MultiplyIfBombCmd {
    fn execute(&self, state: &mut GameState<DouDizhuCard>) {
        // apply_action 已将本次打出的牌追加到 table_cards 末尾
        let n = self.card_ids.len();
        let cards: Vec<DouDizhuCard> = if state.table_cards.len() >= n {
            state.table_cards[state.table_cards.len() - n..].to_vec()
        } else {
            return;
        };
        if let Some(p) = PatternRecognizer::recognize(&cards) {
            let m = match p.pattern_type {
                PatternType::Bomb => self.bomb_multiplier,
                PatternType::Rocket => self.rocket_multiplier,
                _ => return,
            };
            if let Ok(mut sb) = self.scoreboard.lock() {
                sb.multiply(m);
            }
        }
    }
}

/// 春天/反春天检测监听器。
///
/// 终局（任一玩家手牌为 0）时检测：
/// - 春天：农民胜且地主只出过 ≤1 手牌
/// - 反春天：地主胜且两农民均未出过牌
///
/// **注意**：须在 [`SettlementListener`] 之前注册，以确保春天倍数在结算前生效。
pub struct SpringListener {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    landlord: u32,
    farmers: [u32; 2],
    enable_spring: bool,
    enable_anti_spring: bool,
    spring_multiplier: u32,
}

impl SpringListener {
    pub fn new(
        scoreboard: Arc<Mutex<ScoreBoard>>,
        landlord: u32,
        farmers: [u32; 2],
        enable_spring: bool,
        enable_anti_spring: bool,
        spring_multiplier: u32,
    ) -> Self {
        Self {
            scoreboard,
            landlord,
            farmers,
            enable_spring,
            enable_anti_spring,
            spring_multiplier,
        }
    }
}

impl EventListener<DouDizhuCard> for SpringListener {
    fn on_event(
        &mut self,
        event: &GameEvent,
    ) -> Vec<Box<dyn GameCommand<DouDizhuCard> + Send + Sync>> {
        if let GameEvent::AfterAction(Action::Play(pid, _)) = event {
            return vec![Box::new(SpringCheckCmd {
                scoreboard: self.scoreboard.clone(),
                landlord: self.landlord,
                farmers: self.farmers,
                winner: *pid,
                enable_spring: self.enable_spring,
                enable_anti_spring: self.enable_anti_spring,
                spring_multiplier: self.spring_multiplier,
            })];
        }
        vec![]
    }
}

#[derive(Debug)]
struct SpringCheckCmd {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    landlord: u32,
    farmers: [u32; 2],
    winner: u32,
    enable_spring: bool,
    enable_anti_spring: bool,
    spring_multiplier: u32,
}

impl GameCommand<DouDizhuCard> for SpringCheckCmd {
    fn execute(&self, state: &mut GameState<DouDizhuCard>) {
        // 终局判定：winner 手牌为 0
        let winner_empty = state
            .players_hands
            .get(&self.winner)
            .map(|h| h.is_empty())
            .unwrap_or(false);
        if !winner_empty {
            return;
        }
        let farmer_win = self.farmers.contains(&self.winner);
        // 统计 history 中各玩家的 Play 次数
        let mut landlord_plays = 0u32;
        let mut farmer_plays = [0u32; 2];
        for a in &state.history {
            if let Action::Play(p, _) = a {
                if *p == self.landlord {
                    landlord_plays += 1;
                } else if *p == self.farmers[0] {
                    farmer_plays[0] += 1;
                } else if *p == self.farmers[1] {
                    farmer_plays[1] += 1;
                }
            }
        }
        if let Ok(mut sb) = self.scoreboard.lock() {
            if farmer_win && self.enable_spring {
                // 春天：地主只出过 ≤1 手
                if landlord_plays <= 1 {
                    sb.apply_spring(self.spring_multiplier);
                }
            }
            if !farmer_win && self.enable_anti_spring {
                // 反春天：地主胜且两农民均未出牌
                if farmer_plays[0] == 0 && farmer_plays[1] == 0 {
                    sb.apply_anti_spring(self.spring_multiplier);
                }
            }
        }
    }
}

/// 终局结算监听器：任一玩家手牌为 0 时结算并写入 player_states.scores。
///
/// **注意**：须在 [`SpringListener`] 之后注册，以确保春天倍数在结算前已生效。
pub struct SettlementListener {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    landlord: u32,
    farmers: [u32; 2],
}

impl SettlementListener {
    pub fn new(scoreboard: Arc<Mutex<ScoreBoard>>, landlord: u32, farmers: [u32; 2]) -> Self {
        Self {
            scoreboard,
            landlord,
            farmers,
        }
    }
}

impl EventListener<DouDizhuCard> for SettlementListener {
    fn on_event(
        &mut self,
        event: &GameEvent,
    ) -> Vec<Box<dyn GameCommand<DouDizhuCard> + Send + Sync>> {
        if let GameEvent::AfterAction(Action::Play(pid, _)) = event {
            return vec![Box::new(SettleCmd {
                scoreboard: self.scoreboard.clone(),
                landlord: self.landlord,
                farmers: self.farmers,
                winner: *pid,
            })];
        }
        vec![]
    }
}

#[derive(Debug)]
struct SettleCmd {
    scoreboard: Arc<Mutex<ScoreBoard>>,
    landlord: u32,
    farmers: [u32; 2],
    winner: u32,
}

impl GameCommand<DouDizhuCard> for SettleCmd {
    fn execute(&self, state: &mut GameState<DouDizhuCard>) {
        let winner_empty = state
            .players_hands
            .get(&self.winner)
            .map(|h| h.is_empty())
            .unwrap_or(false);
        if !winner_empty {
            return;
        }
        let side = if self.winner == self.landlord {
            Side::Landlord
        } else {
            Side::Farmer
        };
        let outcome = {
            let sb = self.scoreboard.lock().expect("scoreboard lock");
            sb.settle(side, self.landlord, self.farmers)
        };
        for (pid, score) in outcome.final_scores {
            *state.player_states.scores.entry(pid).or_insert(0) = score;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Deck, Hand};
    use crate::engine::{Action, GameEngine, Phase};
    use crate::presets::dou_dizhu::card::{DouDizhuCard, DouDizhuRank};
    use crate::rule::{ActionValidator, GameContext, PlayerStatesView};
    use std::collections::{HashMap, VecDeque};

    /// 永远返回 `true` 的校验器，用于绕过规则校验。
    struct AlwaysValidator;
    impl ActionValidator<DouDizhuCard> for AlwaysValidator {
        fn validate(&self, _: &GameContext<'_, DouDizhuCard>, _: &Action) -> bool {
            true
        }
    }

    fn c(id: u32, r: DouDizhuRank) -> DouDizhuCard {
        DouDizhuCard::new(id, r)
    }

    /// 构造最小可用状态：指定玩家手牌与桌面牌。
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

    #[test]
    fn clear_table_cmd_clears_table() {
        let mut state = make_state(
            HashMap::new(),
            vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Six)],
            1,
        );
        ClearTableCmd.execute(&mut state);
        assert!(state.table_cards.is_empty());
    }

    #[test]
    fn award_score_cmd_adds_score() {
        let mut state = make_state(HashMap::new(), vec![], 1);
        AwardScoreCmd {
            player: 1,
            delta: 10,
        }
        .execute(&mut state);
        assert_eq!(state.player_states.scores.get(&1), Some(&10));
    }

    #[test]
    fn round_end_listener_clears_after_two_passes() {
        let mut hands = HashMap::new();
        hands.insert(1u32, vec![c(0, DouDizhuRank::Five)]);
        hands.insert(2u32, vec![c(1, DouDizhuRank::Six)]);
        let state = make_state(hands, vec![], 1);
        let mut engine = GameEngine::new(AlwaysValidator, state);
        engine.register_listener(RoundEndListener::default());

        // Play → table_cards 非空
        engine.transition(Action::Play(1, vec![0])).unwrap();
        assert_eq!(engine.state.table_cards.len(), 1);

        // Pass → 1 次
        engine.transition(Action::Pass(2)).unwrap();
        assert_eq!(engine.state.table_cards.len(), 1);

        // Pass → 2 次 → 清桌
        engine.transition(Action::Pass(1)).unwrap();
        assert!(engine.state.table_cards.is_empty());
    }

    #[test]
    fn bomb_multiplier_listener_doubles_on_bomb() {
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
        let mut engine = GameEngine::new(AlwaysValidator, state);
        engine.register_listener(BombMultiplierListener::new(scoreboard.clone(), 2, 2));

        engine
            .transition(Action::Play(1, vec![0, 1, 2, 3]))
            .unwrap();

        let sb = scoreboard.lock().expect("scoreboard lock");
        assert_eq!(sb.multiplier(), 2);
    }

    #[test]
    fn settlement_listener_writes_scores_on_terminal() {
        let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(1)));
        let mut hands = HashMap::new();
        // 地主 1 持有一对 Five
        hands.insert(
            1u32,
            vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Five)],
        );
        hands.insert(2u32, vec![c(2, DouDizhuRank::Three)]);
        hands.insert(3u32, vec![c(3, DouDizhuRank::Three)]);
        let state = make_state(hands, vec![], 1);
        let mut engine = GameEngine::new(AlwaysValidator, state);
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
        engine.transition(Action::Play(1, vec![0, 1])).unwrap();

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
}
