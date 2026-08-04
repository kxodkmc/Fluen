//! 斗地主牌型识别。
//!
//! 设计要点：
//! - [`PatternRecognizer`] 为纯函数式识别器，可独立复用与包装扩展
//! - 覆盖主流斗地主全部 13 种牌型
//! - 顺子/连对/飞机不含 2 与王
//! - 飞机带翅膀要求翅膀数量与飞机组数严格相等

use crate::presets::dou_dizhu::card::{DouDizhuCard, DouDizhuRank};
use std::collections::BTreeMap;

/// 13 种斗地主牌型。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternType {
    Single = 1,
    Pair = 2,
    Triple = 3,
    TripleSingle = 4,
    TriplePair = 5,
    Straight = 6,
    PairStraight = 7,
    Plane = 8,
    PlaneSingle = 9,
    PlanePair = 10,
    FourTwo = 11,
    FourTwoPair = 12,
    Bomb = 13,
    Rocket = 14,
}

impl PatternType {
    /// 转为 `u32`，用于映射到框架的 `HandValue.rank`。
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// 牌型识别结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandPattern {
    pub pattern_type: PatternType,
    /// 主牌点数：单/对/三/三带/炸弹 → 该点数；顺子/连对/飞机 → 起始点数；王炸 → RedJoker
    pub main_rank: DouDizhuRank,
    /// 长度：顺子张数 / 连对对数 / 飞机组数；其余为 1
    pub length: usize,
    /// 翅膀点数列表（飞机带翅膀、四带二的副牌），其余为空
    pub kickers: Vec<DouDizhuRank>,
}

/// 纯函数式牌型识别器。
///
/// 可被包装扩展自定义牌型：下游实现自己的 `recognize`，先调用 `PatternRecognizer::recognize`，
/// 失败后追加变体识别逻辑。
#[derive(Debug, Clone, Copy, Default)]
pub struct PatternRecognizer;

impl PatternRecognizer {
    pub fn recognize(cards: &[DouDizhuCard]) -> Option<HandPattern> {
        if cards.is_empty() {
            return None;
        }
        // 按点数计数
        let mut counts: BTreeMap<DouDizhuRank, usize> = BTreeMap::new();
        for c in cards {
            *counts.entry(c.rank).or_insert(0) += 1;
        }
        let n = cards.len();

        // 王炸：大王+小王
        if n == 2 {
            let ranks: Vec<DouDizhuRank> = counts.keys().copied().collect();
            if ranks.len() == 2
                && ranks.contains(&DouDizhuRank::BlackJoker)
                && ranks.contains(&DouDizhuRank::RedJoker)
            {
                return Some(HandPattern {
                    pattern_type: PatternType::Rocket,
                    main_rank: DouDizhuRank::RedJoker,
                    length: 1,
                    kickers: vec![],
                });
            }
        }

        // 单一牌型（仅一种点数）
        if counts.len() == 1 {
            let (&rank, &cnt) = counts.iter().next().unwrap();
            return match cnt {
                1 => Some(HandPattern {
                    pattern_type: PatternType::Single,
                    main_rank: rank,
                    length: 1,
                    kickers: vec![],
                }),
                2 => Some(HandPattern {
                    pattern_type: PatternType::Pair,
                    main_rank: rank,
                    length: 1,
                    kickers: vec![],
                }),
                3 => Some(HandPattern {
                    pattern_type: PatternType::Triple,
                    main_rank: rank,
                    length: 1,
                    kickers: vec![],
                }),
                4 => Some(HandPattern {
                    pattern_type: PatternType::Bomb,
                    main_rank: rank,
                    length: 1,
                    kickers: vec![],
                }),
                _ => None,
            };
        }

        // 火箭已处理；下面处理复合牌型
        // 收集按计数分组：triples / pairs / singles（不含王，王不能参与三带/顺子/飞机）
        let mut triples: Vec<DouDizhuRank> = vec![];
        let mut pairs: Vec<DouDizhuRank> = vec![];
        let mut singles: Vec<DouDizhuRank> = vec![];
        let mut quads: Vec<DouDizhuRank> = vec![];
        for (&rank, &cnt) in &counts {
            match cnt {
                1 => singles.push(rank),
                2 => pairs.push(rank),
                3 => triples.push(rank),
                4 => quads.push(rank),
                _ => {}
            }
        }

        // 三带一 / 三带一对（单一三张 + 1 单或 1 对）
        if triples.len() == 1 && quads.is_empty() {
            let main = triples[0];
            if singles.len() == 1 && pairs.is_empty() && n == 4 {
                return Some(HandPattern {
                    pattern_type: PatternType::TripleSingle,
                    main_rank: main,
                    length: 1,
                    kickers: vec![singles[0]],
                });
            }
            if pairs.len() == 1 && singles.is_empty() && n == 5 {
                return Some(HandPattern {
                    pattern_type: PatternType::TriplePair,
                    main_rank: main,
                    length: 1,
                    kickers: vec![pairs[0]],
                });
            }
        }

        // 四带二（4 张相同 + 2 单牌）或四带两对（4 张相同 + 2 对）
        if quads.len() == 1 {
            let main = quads[0];
            // 四带二单
            if singles.len() == 2 && pairs.is_empty() && triples.is_empty() && n == 6 {
                return Some(HandPattern {
                    pattern_type: PatternType::FourTwo,
                    main_rank: main,
                    length: 1,
                    kickers: singles.clone(),
                });
            }
            // 四带两对
            if pairs.len() == 2 && singles.is_empty() && triples.is_empty() && n == 8 {
                return Some(HandPattern {
                    pattern_type: PatternType::FourTwoPair,
                    main_rank: main,
                    length: 1,
                    kickers: pairs.clone(),
                });
            }
        }

        // 顺子：≥5 连续单牌，不含 2 与王
        if singles.len() == counts.len()
            && n >= 5
            && triples.is_empty()
            && pairs.is_empty()
            && quads.is_empty()
            && let Some(start) = check_straight(&singles, false)
        {
            return Some(HandPattern {
                pattern_type: PatternType::Straight,
                main_rank: start,
                length: n,
                kickers: vec![],
            });
        }

        // 连对：≥3 连续对子，不含 2 与王
        if pairs.len() == counts.len()
            && n >= 6
            && triples.is_empty()
            && singles.is_empty()
            && quads.is_empty()
            && let Some(start) = check_straight(&pairs, false)
        {
            return Some(HandPattern {
                pattern_type: PatternType::PairStraight,
                main_rank: start,
                length: pairs.len(),
                kickers: vec![],
            });
        }

        // 飞机及其带翅膀
        if !triples.is_empty() && quads.is_empty() {
            // 飞机不带翅膀：连续三张 ≥2
            if singles.is_empty()
                && pairs.is_empty()
                && triples.len() >= 2
                && let Some(start) = check_straight(&triples, false)
                && n == triples.len() * 3
            {
                return Some(HandPattern {
                    pattern_type: PatternType::Plane,
                    main_rank: start,
                    length: triples.len(),
                    kickers: vec![],
                });
            }
            // 飞机带单：连续三张 + 等量单牌（单牌数 == 三张组数）
            if triples.len() >= 2
                && pairs.is_empty()
                && let Some(start) = check_straight(&triples, false)
            {
                let plane_count = triples.len();
                // 翅膀总数 == plane_count，翅膀来自 singles（每个 1 张）；
                // 注意：翅膀不能与三张点数重叠（已天然不重叠，因 counts 分组）
                if singles.len() == plane_count && n == plane_count * 4 {
                    return Some(HandPattern {
                        pattern_type: PatternType::PlaneSingle,
                        main_rank: start,
                        length: plane_count,
                        kickers: singles.clone(),
                    });
                }
            }
            // 飞机带对：连续三张 + 等量对子
            if triples.len() >= 2
                && singles.is_empty()
                && let Some(start) = check_straight(&triples, false)
            {
                let plane_count = triples.len();
                if pairs.len() == plane_count && n == plane_count * 5 {
                    return Some(HandPattern {
                        pattern_type: PatternType::PlanePair,
                        main_rank: start,
                        length: plane_count,
                        kickers: pairs.clone(),
                    });
                }
            }
        }

        None
    }
}

/// 检查点数序列是否构成不含 2/王的连续序列。
///
/// `ranks` 已是去重后的点数列表。返回起始点数（最小），不连续或含 2/王返回 `None`。
fn check_straight(ranks: &[DouDizhuRank], _allow_two: bool) -> Option<DouDizhuRank> {
    if ranks.len() < 2 {
        return None;
    }
    let mut sorted: Vec<DouDizhuRank> = ranks.to_vec();
    sorted.sort();
    // 不允许含 2 或王
    for r in &sorted {
        if r.value() >= DouDizhuRank::Two.value() {
            return None;
        }
    }
    // 检查连续
    for i in 1..sorted.len() {
        if sorted[i].value() != sorted[i - 1].value() + 1 {
            return None;
        }
    }
    Some(sorted[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: u32, r: DouDizhuRank) -> DouDizhuCard {
        DouDizhuCard::new(id, r)
    }

    #[test]
    fn single_pair_triple_bomb() {
        let p = PatternRecognizer::recognize(&[c(0, DouDizhuRank::Five)]).unwrap();
        assert_eq!(p.pattern_type, PatternType::Single);
        assert_eq!(p.main_rank, DouDizhuRank::Five);

        let p = PatternRecognizer::recognize(&[c(0, DouDizhuRank::Five), c(1, DouDizhuRank::Five)])
            .unwrap();
        assert_eq!(p.pattern_type, PatternType::Pair);

        let p = PatternRecognizer::recognize(&[
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
        ])
        .unwrap();
        assert_eq!(p.pattern_type, PatternType::Triple);

        let p = PatternRecognizer::recognize(&[
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
        ])
        .unwrap();
        assert_eq!(p.pattern_type, PatternType::Bomb);
    }

    #[test]
    fn rocket() {
        let p = PatternRecognizer::recognize(&[
            c(0, DouDizhuRank::BlackJoker),
            c(1, DouDizhuRank::RedJoker),
        ])
        .unwrap();
        assert_eq!(p.pattern_type, PatternType::Rocket);
        assert_eq!(p.main_rank, DouDizhuRank::RedJoker);
    }

    #[test]
    fn triple_single_and_pair() {
        let cards = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Seven),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::TripleSingle);
        assert_eq!(p.main_rank, DouDizhuRank::Five);
        assert_eq!(p.kickers, vec![DouDizhuRank::Seven]);

        let cards = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Seven),
            c(4, DouDizhuRank::Seven),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::TriplePair);
    }

    #[test]
    fn straight() {
        let cards: Vec<DouDizhuCard> = (3..8).map(|i| c(i, rank_by_value(i as u8))).collect();
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::Straight);
        assert_eq!(p.length, 5);
        assert_eq!(p.main_rank, DouDizhuRank::Three);

        // 10-A 顺子
        let cards: Vec<DouDizhuCard> = [10u8, 11, 12, 13, 14]
            .iter()
            .enumerate()
            .map(|(i, &v)| c(i as u32, rank_by_value(v)))
            .collect();
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::Straight);
        assert_eq!(p.main_rank, rank_by_value(10));
    }

    #[test]
    fn straight_cannot_contain_two_or_joker() {
        // A,2 不能构成顺子（2 不可入顺子）
        let cards: Vec<DouDizhuCard> = [14u8, 15]
            .iter()
            .enumerate()
            .map(|(i, &v)| c(i as u32, rank_by_value(v)))
            .collect();
        assert!(PatternRecognizer::recognize(&cards).is_none());
    }

    #[test]
    fn pair_straight() {
        // 3,3,4,4,5,5
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Four),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Five),
            c(5, DouDizhuRank::Five),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::PairStraight);
        assert_eq!(p.length, 3);
    }

    #[test]
    fn plane_no_wings() {
        // 3,3,3,4,4,4
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Three),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Four),
            c(5, DouDizhuRank::Four),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::Plane);
        assert_eq!(p.length, 2);
        assert_eq!(p.main_rank, DouDizhuRank::Three);
    }

    #[test]
    fn plane_single() {
        // 3,3,3,4,4,4,5,6
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Three),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Four),
            c(5, DouDizhuRank::Four),
            c(6, DouDizhuRank::Five),
            c(7, DouDizhuRank::Six),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::PlaneSingle);
        assert_eq!(p.length, 2);
        assert_eq!(p.main_rank, DouDizhuRank::Three);
    }

    #[test]
    fn plane_pair() {
        // 3,3,3,4,4,4,5,5,6,6
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Three),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Four),
            c(5, DouDizhuRank::Four),
            c(6, DouDizhuRank::Five),
            c(7, DouDizhuRank::Five),
            c(8, DouDizhuRank::Six),
            c(9, DouDizhuRank::Six),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::PlanePair);
        assert_eq!(p.length, 2);
    }

    #[test]
    fn four_two_single() {
        // 5,5,5,5,7,8
        let cards = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
            c(4, DouDizhuRank::Seven),
            c(5, DouDizhuRank::Eight),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::FourTwo);
    }

    #[test]
    fn four_two_pair() {
        // 5,5,5,5,7,7,8,8
        let cards = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
            c(4, DouDizhuRank::Seven),
            c(5, DouDizhuRank::Seven),
            c(6, DouDizhuRank::Eight),
            c(7, DouDizhuRank::Eight),
        ];
        let p = PatternRecognizer::recognize(&cards).unwrap();
        assert_eq!(p.pattern_type, PatternType::FourTwoPair);
    }

    #[test]
    fn invalid_combinations() {
        // 3,3,4,4,5 非任何牌型
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Four),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Five),
        ];
        assert!(PatternRecognizer::recognize(&cards).is_none());

        // 空牌组
        assert!(PatternRecognizer::recognize(&[]).is_none());

        // 5 张相同（非法）
        let cards = vec![
            c(0, DouDizhuRank::Five),
            c(1, DouDizhuRank::Five),
            c(2, DouDizhuRank::Five),
            c(3, DouDizhuRank::Five),
            c(4, DouDizhuRank::Five),
        ];
        assert!(PatternRecognizer::recognize(&cards).is_none());
    }

    #[test]
    fn plane_wings_count_mismatch_rejected() {
        // 3,3,3,4,4,4,5（飞机带单但翅膀数 != 组数）
        let cards = vec![
            c(0, DouDizhuRank::Three),
            c(1, DouDizhuRank::Three),
            c(2, DouDizhuRank::Three),
            c(3, DouDizhuRank::Four),
            c(4, DouDizhuRank::Four),
            c(5, DouDizhuRank::Four),
            c(6, DouDizhuRank::Five),
        ];
        assert!(PatternRecognizer::recognize(&cards).is_none());
    }

    /// 辅助：按数值构造 `DouDizhuRank`（仅用于测试）。
    fn rank_by_value(v: u8) -> DouDizhuRank {
        use DouDizhuRank::*;
        match v {
            3 => Three,
            4 => Four,
            5 => Five,
            6 => Six,
            7 => Seven,
            8 => Eight,
            9 => Nine,
            10 => Ten,
            11 => Jack,
            12 => Queen,
            13 => King,
            14 => Ace,
            15 => Two,
            16 => BlackJoker,
            17 => RedJoker,
            _ => panic!("invalid rank value {v}"),
        }
    }
}
