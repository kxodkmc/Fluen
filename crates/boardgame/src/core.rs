//! Core 层 — 牌堆 / 手牌等核心数据结构。
//!
//! 设计要点：
//! - [`Deck`] 支持 Fisher–Yates 洗牌，可注入确定性 RNG 便于测试与回放
//! - [`Hand`] 使用 `Vec` 维持摸牌先后顺序，并提供 `sort_by` 由下游决定排序规则
//! - 错误统一由 [`CoreError`] 表达

use crate::entity::CardEntity;
use rand::Rng;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    CardNotFound,
    DeckEmpty,
}

/// 牌堆：支持注入确定性随机数生成器
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck<C: CardEntity> {
    cards: Vec<C>,
}

impl<C: CardEntity> Deck<C> {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn from_vec(cards: Vec<C>) -> Self {
        Self { cards }
    }

    pub fn push(&mut self, card: C) {
        self.cards.push(card);
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn iter(&self) -> &[C] {
        &self.cards
    }

    /// Fisher–Yates 洗牌，可注入确定性 RNG
    pub fn shuffle<R: Rng>(&mut self, rng: &mut R) {
        for i in (1..self.cards.len()).rev() {
            let j = rng.random_range(0..=i);
            self.cards.swap(i, j);
        }
    }

    pub fn deal(&mut self) -> Option<C> {
        self.cards.pop()
    }
}

impl<C: CardEntity> Default for Deck<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// 手牌：使用 Vec 维持插入顺序（摸牌先后），支持按需排序
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand<C: CardEntity> {
    cards: Vec<C>,
}

impl<C: CardEntity> Hand<C> {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn add(&mut self, card: C) {
        self.cards.push(card);
    }

    pub fn remove(&mut self, id: u32) -> Result<C, CoreError> {
        if let Some(pos) = self.cards.iter().position(|c| c.id() == id) {
            Ok(self.cards.remove(pos))
        } else {
            Err(CoreError::CardNotFound)
        }
    }

    pub fn contains(&self, id: u32) -> bool {
        self.cards.iter().any(|c| c.id() == id)
    }

    pub fn iter(&self) -> &[C] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// 允许下游传入自定义排序闭包（如按点数排序用于 UI 展示）
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&C, &C) -> std::cmp::Ordering,
    {
        self.cards.sort_by(compare);
    }
}

impl<C: CardEntity> Default for Hand<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{StandardCard, StandardSuit};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// 构建一个测试用的小牌堆。
    fn sample_deck() -> Deck<StandardCard> {
        let cards = StandardCard::standard_deck();
        Deck::from_vec(cards)
    }

    /// 确定性洗牌：相同种子 + 相同初始牌堆 → 相同结果。
    #[test]
    fn shuffle_is_deterministic_with_same_seed() {
        let mut deck_a = sample_deck();
        let mut deck_b = sample_deck();
        assert_eq!(deck_a, deck_b);

        let mut rng_a = StdRng::seed_from_u64(42);
        let mut rng_b = StdRng::seed_from_u64(42);
        deck_a.shuffle(&mut rng_a);
        deck_b.shuffle(&mut rng_b);

        assert_eq!(deck_a, deck_b);
    }

    /// 不同种子通常产生不同结果（概率意义上）。
    #[test]
    fn shuffle_differs_with_different_seed() {
        let mut deck_a = sample_deck();
        let mut deck_b = sample_deck();

        let mut rng_a = StdRng::seed_from_u64(42);
        let mut rng_b = StdRng::seed_from_u64(7);
        deck_a.shuffle(&mut rng_a);
        deck_b.shuffle(&mut rng_b);

        assert_ne!(deck_a, deck_b);
    }

    /// 空牌堆 `deal` 返回 `None`。
    #[test]
    fn deal_empty_deck_returns_none() {
        let mut deck: Deck<StandardCard> = Deck::new();
        assert!(deck.is_empty());
        assert!(deck.deal().is_none());
    }

    /// `deal` 从顶部（尾部）取牌。
    #[test]
    fn deal_pops_from_top() {
        let mut deck = Deck::from_vec(vec![
            StandardCard::new(0, StandardSuit::Spade, 3),
            StandardCard::new(1, StandardSuit::Spade, 4),
        ]);
        let dealt = deck.deal();
        assert_eq!(dealt.unwrap().id(), 1);
        assert_eq!(deck.len(), 1);
    }

    /// `Hand` 保留插入顺序。
    #[test]
    fn hand_preserves_insertion_order() {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(0, StandardSuit::Spade, 3));
        hand.add(StandardCard::new(1, StandardSuit::Heart, 5));
        hand.add(StandardCard::new(2, StandardSuit::Club, 7));

        let ids: Vec<u32> = hand.iter().iter().map(|c| c.id()).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    /// `Hand::sort_by` 按指定规则重排。
    #[test]
    fn hand_sort_by_reorders() {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(0, StandardSuit::Spade, 5));
        hand.add(StandardCard::new(1, StandardSuit::Heart, 3));
        hand.add(StandardCard::new(2, StandardSuit::Club, 9));

        hand.sort_by(|a, b| a.rank().cmp(&b.rank()));

        let ranks: Vec<u8> = hand.iter().iter().map(|c| c.rank()).collect();
        assert_eq!(ranks, vec![3, 5, 9]);
    }

    /// `Hand::remove` 找不到 id 时返回 `Err(CoreError::CardNotFound)`。
    #[test]
    fn hand_remove_missing_returns_err() {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(0, StandardSuit::Spade, 3));

        let result = hand.remove(999);
        assert!(matches!(result, Err(CoreError::CardNotFound)));
        assert_eq!(hand.len(), 1);
    }

    /// `Hand::remove` 成功时移除并返回对应卡牌。
    #[test]
    fn hand_remove_existing_succeeds() {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(0, StandardSuit::Spade, 3));
        hand.add(StandardCard::new(1, StandardSuit::Heart, 5));

        let removed = hand.remove(0);
        assert!(removed.is_ok());
        assert_eq!(removed.unwrap().id(), 0);
        assert_eq!(hand.len(), 1);
        assert!(!hand.contains(0));
    }

    /// `Hand::contains` 行为正确。
    #[test]
    fn hand_contains_works() {
        let mut hand = Hand::new();
        hand.add(StandardCard::new(42, StandardSuit::Diamond, 10));
        assert!(hand.contains(42));
        assert!(!hand.contains(43));
    }

    /// `Deck::default` 与 `Deck::new` 等价。
    #[test]
    fn deck_default_is_empty() {
        let deck: Deck<StandardCard> = Deck::default();
        assert!(deck.is_empty());
        assert_eq!(deck.len(), 0);
    }

    /// `Hand::default` 与 `Hand::new` 等价。
    #[test]
    fn hand_default_is_empty() {
        let hand: Hand<StandardCard> = Hand::default();
        assert!(hand.is_empty());
        assert_eq!(hand.len(), 0);
    }

    /// `CoreError` 相等性。
    #[test]
    fn core_error_equality() {
        assert_eq!(CoreError::CardNotFound, CoreError::CardNotFound);
        assert_ne!(CoreError::CardNotFound, CoreError::DeckEmpty);
    }
}
