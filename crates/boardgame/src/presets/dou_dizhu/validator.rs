//! 斗地主动作校验器。
//!
//! 校验规则：
//! 1. `Play` 的玩家须为当前轮次玩家
//! 2. 须持有 `card_ids` 中的所有牌（且无重复 id）
//! 3. 牌组能被识别为合法牌型
//! 4. 若有上家牌（`table_cards` 非空），须能压过上家牌型
//! 5. `Pass` 始终合法

use crate::engine::Action;
use crate::entity::CardEntity;
use crate::presets::dou_dizhu::card::DouDizhuCard;
use crate::presets::dou_dizhu::comparator::DouDizhuComparator;
use crate::presets::dou_dizhu::evaluator::DouDizhuEvaluator;
use crate::rule::{ActionValidator, GameContext, HandEvaluator, ValueComparator};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// 斗地主动作校验器：实现框架的 [`ActionValidator`]。
#[derive(Debug, Clone, Default)]
pub struct DouDizhuValidator {
    evaluator: DouDizhuEvaluator,
    comparator: DouDizhuComparator,
}

impl DouDizhuValidator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ActionValidator<DouDizhuCard> for DouDizhuValidator {
    fn validate(&self, ctx: &GameContext<'_, DouDizhuCard>, action: &Action) -> bool {
        match action {
            Action::Pass(_) => true,
            Action::Play(pid, card_ids) => {
                // 1. 轮次校验
                if *pid != ctx.current_player {
                    return false;
                }
                // 2. 持牌校验：所有 card_ids 须在当前玩家手牌中
                let hand_ids: HashSet<u32> = ctx.current_hand.iter().map(|c| c.id()).collect();
                if card_ids.iter().any(|id| !hand_ids.contains(id)) {
                    return false;
                }
                // 防止 card_ids 中有重复 id（手牌中每张牌唯一）
                let unique_ids: HashSet<u32> = card_ids.iter().copied().collect();
                if unique_ids.len() != card_ids.len() {
                    return false;
                }
                // 3. 取出实际牌实体并识别牌型
                let id_to_card: HashMap<u32, &DouDizhuCard> =
                    ctx.current_hand.iter().map(|c| (c.id(), c)).collect();
                let cards: Vec<DouDizhuCard> = card_ids
                    .iter()
                    .map(|id| **id_to_card.get(id).expect("checked above"))
                    .collect();
                let new_val = match self.evaluator.evaluate(ctx, &cards) {
                    Some(v) => v,
                    None => return false,
                };
                // 4. 压牌校验：若桌面有上家牌，须能压过
                if !ctx.table_cards.is_empty() {
                    let prev_val = match self.evaluator.evaluate(ctx, ctx.table_cards) {
                        Some(v) => v,
                        None => return false, // 上家牌型不合法（不应发生）
                    };
                    if self.comparator.compare(&new_val, &prev_val) != Ordering::Greater {
                        return false;
                    }
                }
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Action, Phase};
    use crate::presets::dou_dizhu::card::{DouDizhuCard, DouDizhuRank};
    use crate::rule::{ActionValidator, GameContext, PlayerStatesView};

    fn c(id: u32, r: DouDizhuRank) -> DouDizhuCard {
        DouDizhuCard::new(id, r)
    }

    fn make_ctx<'a>(
        current_player: u32,
        hand: &'a [DouDizhuCard],
        table: &'a [DouDizhuCard],
        ps: &'a PlayerStatesView,
    ) -> GameContext<'a, DouDizhuCard> {
        GameContext {
            current_player,
            phase: Phase::Playing,
            history: &[],
            table_cards: table,
            current_hand: hand,
            players_state: ps,
            played_cards: &[],
        }
    }

    fn default_ps(player: u32, hand_len: usize) -> PlayerStatesView {
        let mut ps = PlayerStatesView::default();
        ps.hands_count.insert(player, hand_len);
        ps
    }

    #[test]
    fn free_play_accepted() {
        // 桌面空，自由出单张 5 → 合法
        let hand = vec![c(0, DouDizhuRank::Five)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(validator.validate(&ctx, &Action::Play(1, vec![0])));
    }

    #[test]
    fn must_be_current_player() {
        // 非当前轮次玩家出牌 → 非法
        let hand = vec![c(0, DouDizhuRank::Five)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(!validator.validate(&ctx, &Action::Play(99, vec![0])));
    }

    #[test]
    fn must_hold_cards() {
        // 手牌中不含 id 999 → 非法
        let hand = vec![c(0, DouDizhuRank::Five)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(!validator.validate(&ctx, &Action::Play(1, vec![999])));
    }

    #[test]
    fn must_be_valid_pattern() {
        // 5 与 7 点数不同，非对子也非任何合法牌型 → 非法
        let hand = vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Seven)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(!validator.validate(&ctx, &Action::Play(1, vec![0, 1])));
    }

    #[test]
    fn beat_same_type_pair() {
        let validator = DouDizhuValidator::new();
        // 桌面：对子 7-7
        let table = vec![c(10, DouDizhuRank::Seven), c(11, DouDizhuRank::Seven)];

        // 出对子 9-9 压过 7-7 → 合法
        {
            let hand = vec![c(0, DouDizhuRank::Nine), c(1, DouDizhuRank::Nine)];
            let ps = default_ps(1, hand.len());
            let ctx = make_ctx(1, &hand, &table, &ps);
            assert!(validator.validate(&ctx, &Action::Play(1, vec![0, 1])));
        }
        // 出对子 5-5 无法压过 7-7 → 非法
        {
            let hand = vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Five)];
            let ps = default_ps(1, hand.len());
            let ctx = make_ctx(1, &hand, &table, &ps);
            assert!(!validator.validate(&ctx, &Action::Play(1, vec![0, 1])));
        }
    }

    #[test]
    fn bomb_beats_non_bomb() {
        // 桌面：顺子 3-4-5-6-7
        let table = vec![
            c(10, DouDizhuRank::Three),
            c(11, DouDizhuRank::Four),
            c(12, DouDizhuRank::Five),
            c(13, DouDizhuRank::Six),
            c(14, DouDizhuRank::Seven),
        ];
        // 出炸弹 9-9-9-9 → 合法（炸弹压非炸弹）
        let hand = vec![
            c(0, DouDizhuRank::Nine),
            c(1, DouDizhuRank::Nine),
            c(2, DouDizhuRank::Nine),
            c(3, DouDizhuRank::Nine),
        ];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(validator.validate(&ctx, &Action::Play(1, vec![0, 1, 2, 3])));
    }

    #[test]
    fn rocket_beats_bomb() {
        // 桌面：炸弹 A-A-A-A
        let table = vec![
            c(10, DouDizhuRank::Ace),
            c(11, DouDizhuRank::Ace),
            c(12, DouDizhuRank::Ace),
            c(13, DouDizhuRank::Ace),
        ];
        // 出王炸 小王+大王 → 合法（王炸压炸弹）
        let hand = vec![c(0, DouDizhuRank::BlackJoker), c(1, DouDizhuRank::RedJoker)];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(validator.validate(&ctx, &Action::Play(1, vec![0, 1])));
    }

    #[test]
    fn straight_length_mismatch_rejected() {
        // 桌面：顺子 3-4-5-6-7（长度 5）
        let table = vec![
            c(10, DouDizhuRank::Three),
            c(11, DouDizhuRank::Four),
            c(12, DouDizhuRank::Five),
            c(13, DouDizhuRank::Six),
            c(14, DouDizhuRank::Seven),
        ];
        // 出顺子 3-4-5-6-7-8（长度 6）：长度不同不可压 → 非法
        let hand = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Four),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Six),
            c(4, DouDizhuRank::Seven),
            c(5, DouDizhuRank::Eight),
        ];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(!validator.validate(&ctx, &Action::Play(1, vec![0, 1, 2, 3, 4, 5])));
    }

    #[test]
    fn duplicate_card_ids_rejected() {
        // card_ids 含重复 id（同一张牌出两次）→ 非法
        let hand = vec![c(0, DouDizhuRank::Five)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(!validator.validate(&ctx, &Action::Play(1, vec![0, 0])));
    }

    #[test]
    fn pass_always_accepted() {
        // Pass 始终合法
        let hand = vec![c(0, DouDizhuRank::Five)];
        let table: Vec<DouDizhuCard> = vec![];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(validator.validate(&ctx, &Action::Pass(1)));
    }

    #[test]
    fn straight_same_length_higher_beats_lower() {
        // 桌面：顺子 3-4-5-6-7（长度 5，起始 3）
        let table = vec![
            c(10, DouDizhuRank::Three),
            c(11, DouDizhuRank::Four),
            c(12, DouDizhuRank::Five),
            c(13, DouDizhuRank::Six),
            c(14, DouDizhuRank::Seven),
        ];
        // 出顺子 4-5-6-7-8（长度 5，起始 4）：同长度比起始点数 → 合法
        let hand = vec![
            c(0, DouDizhuRank::Four),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Six),
            c(3, DouDizhuRank::Seven),
            c(4, DouDizhuRank::Eight),
        ];
        let ps = default_ps(1, hand.len());
        let ctx = make_ctx(1, &hand, &table, &ps);
        let validator = DouDizhuValidator::new();
        assert!(validator.validate(&ctx, &Action::Play(1, vec![0, 1, 2, 3, 4])));
    }
}
