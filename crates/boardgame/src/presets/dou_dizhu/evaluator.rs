//! 斗地主牌型评估器。
//!
//! 将 [`PatternRecognizer`] 的识别结果映射为框架通用的 [`HandValue`]，
//! 使下游可复用框架的比牌基础设施。
//!
//! 连续牌型（顺子/连对/飞机及其带翅膀）将 `length` 编码为 `kickers` 首位，
//! 以便比较器判定「同长度才可比」。

use crate::presets::dou_dizhu::card::DouDizhuCard;
use crate::presets::dou_dizhu::pattern::{HandPattern, PatternRecognizer, PatternType};
use crate::rule::{HandEvaluator, HandValue};

/// 斗地主牌型评估器：实现框架的 [`HandEvaluator`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct DouDizhuEvaluator;

impl DouDizhuEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// 直接评估 `HandPattern` 为 `HandValue`（供比较器/校验器复用）。
    pub fn from_pattern(pattern: &HandPattern) -> HandValue {
        use PatternType::*;
        let mut kickers: Vec<u32> = pattern.kickers.iter().map(|r| r.value() as u32).collect();
        // 对连续牌型，将 length 编码为 kickers 首位，便于比较器判定「同长度才可比」
        if matches!(
            pattern.pattern_type,
            Straight | PairStraight | Plane | PlaneSingle | PlanePair
        ) {
            kickers.insert(0, pattern.length as u32);
        }
        HandValue {
            rank: pattern.pattern_type.as_u32(),
            primary_value: pattern.main_rank.value() as u32,
            kickers,
        }
    }
}

impl HandEvaluator<DouDizhuCard> for DouDizhuEvaluator {
    fn evaluate(
        &self,
        _ctx: &crate::rule::GameContext<'_, DouDizhuCard>,
        cards: &[DouDizhuCard],
    ) -> Option<HandValue> {
        PatternRecognizer::recognize(cards).map(|p| Self::from_pattern(&p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Phase;
    use crate::presets::dou_dizhu::card::{DouDizhuCard, DouDizhuRank};
    use crate::presets::dou_dizhu::pattern::PatternType;
    use crate::rule::{GameContext, HandEvaluator, PlayerStatesView};

    fn c(id: u32, r: DouDizhuRank) -> DouDizhuCard {
        DouDizhuCard::new(id, r)
    }

    fn make_ctx<'a>(
        hand: &'a [DouDizhuCard],
        table: &'a [DouDizhuCard],
        ps: &'a PlayerStatesView,
    ) -> GameContext<'a, DouDizhuCard> {
        GameContext {
            current_player: 1,
            phase: Phase::Playing,
            history: &[],
            table_cards: table,
            current_hand: hand,
            players_state: ps,
            played_cards: &[],
        }
    }

    #[test]
    fn evaluate_maps_bomb() {
        let evaluator = DouDizhuEvaluator::new();
        let hand = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
        ];
        let ps = PlayerStatesView::default();
        let ctx = make_ctx(&hand, &[], &ps);
        let val = evaluator.evaluate(&ctx, &hand).unwrap();
        assert_eq!(val.rank, PatternType::Bomb.as_u32());
        assert_eq!(val.primary_value, DouDizhuRank::Five.value() as u32);
        assert!(val.kickers.is_empty());
    }

    #[test]
    fn evaluate_returns_none_for_invalid() {
        let evaluator = DouDizhuEvaluator::new();
        // 5 与 7 点数不同，既非对子也非任何合法牌型
        let hand = vec![c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Seven)];
        let ps = PlayerStatesView::default();
        let ctx = make_ctx(&hand, &[], &ps);
        assert!(evaluator.evaluate(&ctx, &hand).is_none());
    }

    #[test]
    fn evaluate_encodes_length_for_straight() {
        let evaluator = DouDizhuEvaluator::new();
        // 顺子 3-4-5-6-7，长度 5
        let hand = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Four),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Six),
            c(4, DouDizhuRank::Seven),
        ];
        let ps = PlayerStatesView::default();
        let ctx = make_ctx(&hand, &[], &ps);
        let val = evaluator.evaluate(&ctx, &hand).unwrap();
        assert_eq!(val.rank, PatternType::Straight.as_u32());
        // kickers 首位为长度编码
        assert_eq!(val.kickers, vec![5]);
    }

    #[test]
    fn evaluate_no_length_encoding_for_non_continuous() {
        let evaluator = DouDizhuEvaluator::new();
        // 对子 9-9，非连续牌型，kickers 应为空
        let hand = vec![c(0, DouDizhuRank::Nine), c(1, DouDizhuRank::Nine)];
        let ps = PlayerStatesView::default();
        let ctx = make_ctx(&hand, &[], &ps);
        let val = evaluator.evaluate(&ctx, &hand).unwrap();
        assert_eq!(val.rank, PatternType::Pair.as_u32());
        assert!(val.kickers.is_empty());
    }
}
