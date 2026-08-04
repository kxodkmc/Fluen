//! 斗地主配置与一键启动。
//!
//! 设计要点：
//! - [`DouDizhuConfig`] 集中管理规则变体（叫分制式、底牌数、倍率、春天开关）
//! - [`DouDizhuSetup::new_game`] 一键完成洗牌、发牌、构造初始 `GameState`
//! - 叫地主阶段独立进行（由调用方驱动 `BiddingSession`），定地主后调用
//!   [`DouDizhuSetup::assign_bottom`] 将底牌交给地主

use crate::core::{Deck, Hand};
use crate::engine::{GameState, Phase};
use crate::entity::CardEntity;
use crate::presets::dou_dizhu::bidding::{BiddingMode, BiddingSession};
use crate::presets::dou_dizhu::card::{DouDizhuCard, standard_deck};
use crate::rule::PlayerStatesView;
use rand::Rng;
use std::collections::HashMap;

/// 斗地主配置。
#[derive(Debug, Clone)]
pub struct DouDizhuConfig {
    pub bidding: BiddingMode,
    pub bottom_cards: usize,
    pub deal_per_player: usize,
    pub enable_spring: bool,
    pub enable_anti_spring: bool,
    pub bomb_multiplier: u32,
    pub rocket_multiplier: u32,
    pub spring_multiplier: u32,
}

impl Default for DouDizhuConfig {
    fn default() -> Self {
        Self {
            bidding: BiddingMode::default(),
            bottom_cards: 3,
            deal_per_player: 17,
            enable_spring: true,
            enable_anti_spring: true,
            bomb_multiplier: 2,
            rocket_multiplier: 2,
            spring_multiplier: 2,
        }
    }
}

/// 一键启动辅助器。
pub struct DouDizhuSetup;

impl DouDizhuSetup {
    /// 发牌并构造初始 `GameState`（phase=Bidding）与叫地主会话。
    ///
    /// 返回 `(BiddingSession, GameState<DouDizhuCard>)`。
    /// 底牌 id 列表同时存入 `BiddingSession` 与 `GameState.deck`（deck 中保留底牌，
    /// 供 `assign_bottom` 取出交给地主）。
    pub fn new_game<R: Rng>(
        config: &DouDizhuConfig,
        player_ids: [u32; 3],
        rng: &mut R,
    ) -> (BiddingSession, GameState<DouDizhuCard>) {
        let mut deck = Deck::from_vec(standard_deck());
        deck.shuffle(rng);

        // 发牌
        let mut hands: HashMap<u32, Hand<DouDizhuCard>> = HashMap::new();
        for &pid in &player_ids {
            hands.insert(pid, Hand::new());
        }
        for _ in 0..config.deal_per_player {
            for &pid in &player_ids {
                if let Some(card) = deck.deal() {
                    hands
                        .get_mut(&pid)
                        .expect("player inserted above")
                        .add(card);
                }
            }
        }
        // 取出底牌
        let bottom: Vec<DouDizhuCard> = (0..config.bottom_cards)
            .filter_map(|_| deck.deal())
            .collect();
        let bottom_ids: Vec<u32> = bottom.iter().map(|c| c.id()).collect();
        // 重新构造 deck 存放底牌（供 assign_bottom 取出交给地主）
        let mut bottom_deck = Deck::new();
        for c in bottom {
            bottom_deck.push(c);
        }

        let mut player_states = PlayerStatesView::default();
        for &pid in &player_ids {
            player_states
                .hands_count
                .insert(pid, config.deal_per_player);
        }

        let state = GameState {
            phase: Phase::Bidding,
            current_player: player_ids[0],
            deck: bottom_deck,
            players_hands: hands,
            table_cards: Vec::new(),
            played_cards: Vec::new(),
            history: std::collections::VecDeque::new(),
            player_states,
        };

        let session = BiddingSession::new(config.bidding, player_ids, bottom_ids);

        (session, state)
    }

    /// 将底牌交给地主：从 state.deck 取出全部底牌加入地主手牌，更新 hands_count，
    /// 切换 phase=Playing，current_player=landlord。
    pub fn assign_bottom(state: &mut GameState<DouDizhuCard>, landlord: u32) {
        let mut drawn = Vec::new();
        while let Some(c) = state.deck.deal() {
            drawn.push(c);
        }
        let n = drawn.len();
        let hand = state.players_hands.entry(landlord).or_default();
        for c in drawn {
            hand.add(c);
        }
        *state.player_states.hands_count.entry(landlord).or_insert(0) += n;
        state.phase = Phase::Playing;
        state.current_player = landlord;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn default_config_matches_mainstream() {
        let c = DouDizhuConfig::default();
        assert_eq!(c.bottom_cards, 3);
        assert_eq!(c.deal_per_player, 17);
        assert_eq!(c.bomb_multiplier, 2);
        assert_eq!(c.rocket_multiplier, 2);
        assert_eq!(c.spring_multiplier, 2);
        assert!(c.enable_spring);
        assert!(c.enable_anti_spring);
    }

    #[test]
    fn new_game_deals_correctly() {
        let config = DouDizhuConfig::default();
        let mut rng = StdRng::seed_from_u64(42);
        let (session, state) = DouDizhuSetup::new_game(&config, [1, 2, 3], &mut rng);
        // 各 17 张
        assert_eq!(state.players_hands.get(&1).expect("player 1").len(), 17);
        assert_eq!(state.players_hands.get(&2).expect("player 2").len(), 17);
        assert_eq!(state.players_hands.get(&3).expect("player 3").len(), 17);
        // 底牌 3 张存于 deck
        assert_eq!(state.deck.len(), 3);
        // phase = Bidding
        assert_eq!(state.phase, Phase::Bidding);
        // session 未开始
        assert_eq!(session.outcome(), None);
    }

    #[test]
    fn new_game_is_deterministic_with_same_seed() {
        let config = DouDizhuConfig::default();
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let (_, state1) = DouDizhuSetup::new_game(&config, [1, 2, 3], &mut rng1);
        let (_, state2) = DouDizhuSetup::new_game(&config, [1, 2, 3], &mut rng2);
        assert_eq!(state1, state2);
    }

    #[test]
    fn assign_bottom_transfers_cards_to_landlord() {
        let config = DouDizhuConfig::default();
        let mut rng = StdRng::seed_from_u64(42);
        let (_, mut state) = DouDizhuSetup::new_game(&config, [1, 2, 3], &mut rng);
        DouDizhuSetup::assign_bottom(&mut state, 1);
        // 地主 17+3=20 张
        assert_eq!(state.players_hands.get(&1).expect("landlord").len(), 20);
        assert_eq!(state.deck.len(), 0);
        assert_eq!(state.phase, Phase::Playing);
        assert_eq!(state.current_player, 1);
    }
}
