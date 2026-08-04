//! Entity 层 — 卡牌实体抽象与标准扑克牌实现。
//!
//! 核心设计：
//! - [`CardEntity`] trait 定义所有卡牌实体的通用约束
//! - 刻意不要求 `Ord`，排序权交由下游业务决定（如 UI 展示按点数排序）
//! - 花色 / 点数使用关联类型，便于扩展自定义卡牌（如 tarot、UNO）
//!
//! 内置实现：
//! - [`StandardSuit`]：标准扑克牌 5 种花色（含 Joker）
//! - [`StandardCard`]：标准扑克牌，rank 编码 3-17（3-Ace, 2, BlackJoker, RedJoker）

use std::fmt::Debug;
use std::hash::Hash;

/// 核心 Trait 约束：所有卡牌实体必须实现此接口。
/// 注意：刻意不要求 `Ord`，排序权交由下游业务决定。
pub trait CardEntity: Clone + PartialEq + Eq + Debug {
    type SuitId: Debug + Copy + Eq + Hash;
    type RankId: Debug + Copy + Eq + Hash;

    fn id(&self) -> u32;
    fn suit(&self) -> Self::SuitId;
    fn rank(&self) -> Self::RankId;
}

/// 标准扑克牌花色
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardSuit {
    Spade = 0,
    Heart = 1,
    Club = 2,
    Diamond = 3,
    Joker = 4,
}

/// 标准扑克牌
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardCard {
    pub id: u32,
    pub suit: StandardSuit,
    /// 3-17 (3-Ace, 2, BlackJoker, RedJoker)
    pub rank: u8,
}

impl StandardCard {
    /// 创建一张标准扑克牌的便捷构造函数。
    pub fn new(id: u32, suit: StandardSuit, rank: u8) -> Self {
        Self { id, suit, rank }
    }

    /// 构建一副 54 张标准扑克牌，id 按确定性顺序分配（0..54）。
    ///
    /// 顺序约定：
    /// - 0..52：四种花色（Spade / Heart / Club / Diamond）各 13 张，rank 3..=15
    ///   （3-Ace 对应 3-14，2 对应 15）
    /// - 52..54：两张 Joker（BlackJoker rank=16，RedJoker rank=17）
    pub fn standard_deck() -> Vec<StandardCard> {
        let mut cards = Vec::with_capacity(54);
        let suits = [
            StandardSuit::Spade,
            StandardSuit::Heart,
            StandardSuit::Club,
            StandardSuit::Diamond,
        ];
        let mut id: u32 = 0;
        // 普通牌：rank 3..=15（3,4,...,Ace,2）
        for suit in suits {
            for rank in 3..=15u8 {
                cards.push(StandardCard::new(id, suit, rank));
                id += 1;
            }
        }
        // 大小王：BlackJoker rank=16，RedJoker rank=17
        cards.push(StandardCard::new(id, StandardSuit::Joker, 16));
        id += 1;
        cards.push(StandardCard::new(id, StandardSuit::Joker, 17));
        debug_assert_eq!(cards.len(), 54);
        cards
    }
}

impl CardEntity for StandardCard {
    type SuitId = StandardSuit;
    type RankId = u8;

    fn id(&self) -> u32 {
        self.id
    }

    fn suit(&self) -> Self::SuitId {
        self.suit
    }

    fn rank(&self) -> Self::RankId {
        self.rank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 `StandardCard` 字段可经 `CardEntity` 方法往返读取。
    #[test]
    fn standard_card_round_trips_through_trait() {
        // 通过泛型函数验证 trait 方法可被外部调用
        fn check_entity<C: CardEntity<SuitId = StandardSuit, RankId = u8>>(
            c: &C,
            id: u32,
            suit: StandardSuit,
            rank: u8,
        ) {
            assert_eq!(c.id(), id);
            assert_eq!(c.suit(), suit);
            assert_eq!(c.rank(), rank);
        }

        let card = StandardCard::new(7, StandardSuit::Heart, 11);
        check_entity(&card, 7, StandardSuit::Heart, 11);
        // 字段与 trait 方法一致
        assert_eq!(card.id, card.id());
        assert_eq!(card.suit, card.suit());
        assert_eq!(card.rank, card.rank());
    }

    /// `standard_deck` 应生成 54 张牌，且 id 为 0..54 无重复。
    #[test]
    fn standard_deck_has_54_unique_ids() {
        let deck = StandardCard::standard_deck();
        assert_eq!(deck.len(), 54);
        let mut ids: Vec<u32> = deck.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        let expected: Vec<u32> = (0..54).collect();
        assert_eq!(ids, expected);
    }

    /// `standard_deck` 中四种花色各 13 张，Joker 2 张。
    #[test]
    fn standard_deck_suit_counts() {
        let deck = StandardCard::standard_deck();
        let spades = deck
            .iter()
            .filter(|c| c.suit() == StandardSuit::Spade)
            .count();
        let hearts = deck
            .iter()
            .filter(|c| c.suit() == StandardSuit::Heart)
            .count();
        let clubs = deck
            .iter()
            .filter(|c| c.suit() == StandardSuit::Club)
            .count();
        let diamonds = deck
            .iter()
            .filter(|c| c.suit() == StandardSuit::Diamond)
            .count();
        let jokers = deck
            .iter()
            .filter(|c| c.suit() == StandardSuit::Joker)
            .count();
        assert_eq!(spades, 13);
        assert_eq!(hearts, 13);
        assert_eq!(clubs, 13);
        assert_eq!(diamonds, 13);
        assert_eq!(jokers, 2);
    }

    /// `StandardCard` 不实现 `Ord`：编译期验证。
    ///
    /// `accept_entity` 仅要求 `CardEntity`，不要求 `Ord`；若 `StandardCard` 必须实现
    /// `Ord` 才能满足 `CardEntity`，则本测试无法编译。该测试通过编译即说明 trait
    /// 约束未错误引入 `Ord`。`StandardCard` 本身也未 derive `Ord`。
    #[test]
    fn standard_card_does_not_require_ord() {
        fn accept_entity<C: CardEntity>(_c: &C) {}
        let card = StandardCard::new(0, StandardSuit::Spade, 3);
        accept_entity(&card);
        // 对照：以下函数要求 `Ord`，若取消注释则因 StandardCard 未实现 Ord 而编译失败。
        // fn accept_ord<C: Ord>(_c: &C) {}
        // accept_ord(&card);
    }
}
