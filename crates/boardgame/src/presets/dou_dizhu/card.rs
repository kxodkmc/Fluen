//! 斗地主卡牌实体。
//!
//! 设计要点：
//! - [`DouDizhuRank`] 为语义化点数枚举（3..=17），值越大牌越大
//! - 花色在斗地主中不影响大小，故 [`DouDizhuCard`] 省略花色字段
//! - 实现 [`CardEntity`]，可无缝接入框架的泛型容器与引擎

use crate::entity::CardEntity;

/// 斗地主点数。
///
/// 值为 3..=17：3..=15 对应 3..2，16=小王，17=大王。
/// 值越大牌越大。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DouDizhuRank {
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
    Two = 15,
    BlackJoker = 16,
    RedJoker = 17,
}

impl DouDizhuRank {
    /// 返回点数对应的数值（3..=17）。
    pub fn value(self) -> u8 {
        self as u8
    }

    /// 是否为王（小王或大王）。
    pub fn is_joker(self) -> bool {
        matches!(self, DouDizhuRank::BlackJoker | DouDizhuRank::RedJoker)
    }
}

/// 斗地主卡牌。
///
/// 花色不影响大小，故仅保留 id 与点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DouDizhuCard {
    pub id: u32,
    pub rank: DouDizhuRank,
}

impl DouDizhuCard {
    pub fn new(id: u32, rank: DouDizhuRank) -> Self {
        Self { id, rank }
    }
}

impl CardEntity for DouDizhuCard {
    type SuitId = ();
    type RankId = DouDizhuRank;

    fn id(&self) -> u32 {
        self.id
    }

    fn suit(&self) -> Self::SuitId {}

    fn rank(&self) -> Self::RankId {
        self.rank
    }
}

/// 构造标准斗地主牌堆（54 张）。
///
/// id 分配：0..48 为 4 副花色的 3..A（每个点数 4 张），
/// 48..52 为 4 个 2，52 为小王，53 为大王。
pub fn standard_deck() -> Vec<DouDizhuCard> {
    use DouDizhuRank::*;
    let normal = [
        Three, Four, Five, Six, Seven, Eight, Nine, Ten, Jack, Queen, King, Ace,
    ];
    let mut cards = Vec::with_capacity(54);
    let mut id = 0u32;
    // 4 花色 × 12 点（3..A）
    for _ in 0..4 {
        for &r in &normal {
            cards.push(DouDizhuCard::new(id, r));
            id += 1;
        }
    }
    // 4 个 2
    for _ in 0..4 {
        cards.push(DouDizhuCard::new(id, Two));
        id += 1;
    }
    // 大小王
    cards.push(DouDizhuCard::new(id, BlackJoker));
    id += 1;
    cards.push(DouDizhuCard::new(id, RedJoker));
    debug_assert_eq!(id, 53);
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_value_order() {
        assert_eq!(DouDizhuRank::Three.value(), 3);
        assert_eq!(DouDizhuRank::Two.value(), 15);
        assert_eq!(DouDizhuRank::BlackJoker.value(), 16);
        assert_eq!(DouDizhuRank::RedJoker.value(), 17);
        assert!(DouDizhuRank::Two.value() < DouDizhuRank::BlackJoker.value());
        assert!(DouDizhuRank::BlackJoker.value() < DouDizhuRank::RedJoker.value());
    }

    #[test]
    fn rank_is_joker() {
        assert!(DouDizhuRank::BlackJoker.is_joker());
        assert!(DouDizhuRank::RedJoker.is_joker());
        assert!(!DouDizhuRank::Two.is_joker());
        assert!(!DouDizhuRank::Three.is_joker());
    }

    #[test]
    fn standard_deck_composition() {
        let deck = standard_deck();
        assert_eq!(deck.len(), 54);
        // id 唯一连续 0..54
        let mut ids: Vec<u32> = deck.iter().map(|c| c.id()).collect();
        ids.sort();
        assert_eq!(ids, (0..54).collect::<Vec<_>>());
        // 4 个 Two
        let two_count = deck.iter().filter(|c| c.rank == DouDizhuRank::Two).count();
        assert_eq!(two_count, 4);
        // 1 个 BlackJoker, 1 个 RedJoker
        assert_eq!(
            deck.iter()
                .filter(|c| c.rank == DouDizhuRank::BlackJoker)
                .count(),
            1
        );
        assert_eq!(
            deck.iter()
                .filter(|c| c.rank == DouDizhuRank::RedJoker)
                .count(),
            1
        );
        // 每个普通点数（3..A）各 4 张
        for r in [DouDizhuRank::Three, DouDizhuRank::Ace, DouDizhuRank::King] {
            assert_eq!(deck.iter().filter(|c| c.rank == r).count(), 4);
        }
    }

    #[test]
    fn card_entity_impl() {
        let card = DouDizhuCard::new(7, DouDizhuRank::Five);
        assert_eq!(card.id(), 7);
        assert_eq!(card.rank(), DouDizhuRank::Five);
        assert_eq!(card.suit(), ());
    }

    /// 编译期验证：`DouDizhuCard` 满足 `CardEntity` 约束且不要求 `Ord`。
    #[allow(dead_code)]
    fn _assert_card_entity() {
        fn _check<C: crate::entity::CardEntity>() {}
        _check::<DouDizhuCard>();
    }
}
