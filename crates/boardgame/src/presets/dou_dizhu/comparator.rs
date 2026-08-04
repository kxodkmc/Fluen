//! 斗地主牌值比较器。
//!
//! 比牌规则：
//! - 王炸（Rocket）> 任意
//! - 炸弹（Bomb）> 非炸弹非王炸；炸弹间比 `primary_value`
//! - 同牌型：连续牌型须同 `length` 才可比，否则不可压；非连续牌型比 `primary_value`
//! - 其余不可压（返回 `Less`）
//!
//! 连续牌型（顺子/连对/飞机及其带翅膀）的 `length` 由评估器编码在
//! `HandValue.kickers` 首位，比较器据此判定「同长度才可比」。

use crate::presets::dou_dizhu::pattern::PatternType;
use crate::rule::{HandValue, ValueComparator};
use std::cmp::Ordering;

/// 斗地主牌值比较器：实现框架的 [`ValueComparator`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct DouDizhuComparator;

impl DouDizhuComparator {
    pub fn new() -> Self {
        Self
    }

    /// 将 `HandValue.rank` 还原为 `PatternType`。
    ///
    /// `rank` 由 `PatternType::as_u32` 生成，值域 1..=14。
    fn pattern_type(val: &HandValue) -> PatternType {
        match val.rank {
            1 => PatternType::Single,
            2 => PatternType::Pair,
            3 => PatternType::Triple,
            4 => PatternType::TripleSingle,
            5 => PatternType::TriplePair,
            6 => PatternType::Straight,
            7 => PatternType::PairStraight,
            8 => PatternType::Plane,
            9 => PatternType::PlaneSingle,
            10 => PatternType::PlanePair,
            11 => PatternType::FourTwo,
            12 => PatternType::FourTwoPair,
            13 => PatternType::Bomb,
            14 => PatternType::Rocket,
            _ => PatternType::Single, // 不应发生：rank 始终由 PatternType 生成
        }
    }
}

impl ValueComparator for DouDizhuComparator {
    fn compare(&self, val_a: &HandValue, val_b: &HandValue) -> Ordering {
        use PatternType::*;
        let pa = Self::pattern_type(val_a);
        let pb = Self::pattern_type(val_b);

        // 王炸最大
        if pa == Rocket && pb != Rocket {
            return Ordering::Greater;
        }
        if pb == Rocket && pa != Rocket {
            return Ordering::Less;
        }
        if pa == Rocket && pb == Rocket {
            return Ordering::Equal;
        }

        // 炸弹压非炸弹
        if pa == Bomb && pb != Bomb {
            return Ordering::Greater;
        }
        if pb == Bomb && pa != Bomb {
            return Ordering::Less;
        }
        if pa == Bomb && pb == Bomb {
            return val_a.primary_value.cmp(&val_b.primary_value);
        }

        // 同牌型：连续牌型须同 length 才可比；非连续仅比 primary_value
        if pa == pb {
            if matches!(
                pa,
                Straight | PairStraight | Plane | PlaneSingle | PlanePair
            ) {
                // 连续牌型：长度不同不可压（返回 Less）；长度相同比起始点数
                let len_a = val_a.kickers.first().copied().unwrap_or(0);
                let len_b = val_b.kickers.first().copied().unwrap_or(0);
                if len_a != len_b {
                    return Ordering::Less; // a 不能压 b（长度不同）
                }
                return val_a.primary_value.cmp(&val_b.primary_value);
            }
            // 非连续牌型：比 primary_value（三带/四带二的 kickers 不影响大小）
            return val_a.primary_value.cmp(&val_b.primary_value);
        }

        // 不同牌型非炸弹非王炸 → 不可压（返回 Less 表示 a 不能压 b）
        Ordering::Less
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::HandValue;
    use std::cmp::Ordering;

    /// 构造 `HandValue` 的便捷函数。
    fn val(rank: u32, primary: u32, kickers: Vec<u32>) -> HandValue {
        HandValue {
            rank,
            primary_value: primary,
            kickers,
        }
    }

    #[test]
    fn bomb_beats_straight() {
        let cmp = DouDizhuComparator::new();
        // Straight rank=6（起始点数 3，长度 5）；Bomb rank=13
        let straight = val(6, 3, vec![5]);
        let bomb = val(13, 9, vec![]);
        assert_eq!(cmp.compare(&bomb, &straight), Ordering::Greater);
        // 反向：顺子不可压炸弹
        assert_eq!(cmp.compare(&straight, &bomb), Ordering::Less);
    }

    #[test]
    fn rocket_beats_bomb() {
        let cmp = DouDizhuComparator::new();
        let bomb = val(13, 14, vec![]);
        let rocket = val(14, 17, vec![]);
        assert_eq!(cmp.compare(&rocket, &bomb), Ordering::Greater);
        // 反向：炸弹不可压王炸
        assert_eq!(cmp.compare(&bomb, &rocket), Ordering::Less);
    }

    #[test]
    fn same_type_compare() {
        let cmp = DouDizhuComparator::new();
        // 对子 rank=2
        let pair7 = val(2, 7, vec![]);
        let pair9 = val(2, 9, vec![]);
        assert_eq!(cmp.compare(&pair9, &pair7), Ordering::Greater);
        let pair5 = val(2, 5, vec![]);
        assert_eq!(cmp.compare(&pair5, &pair7), Ordering::Less);
        // 同点数对子相等
        assert_eq!(cmp.compare(&pair7, &pair7), Ordering::Equal);
    }

    #[test]
    fn different_type_not_beatable() {
        let cmp = DouDizhuComparator::new();
        // 单张 rank=1 不可压对子 rank=2
        let single = val(1, 14, vec![]);
        let pair = val(2, 7, vec![]);
        assert_eq!(cmp.compare(&single, &pair), Ordering::Less);
    }

    #[test]
    fn straight_length_mismatch_not_beatable() {
        let cmp = DouDizhuComparator::new();
        // 顺子长度 5（起始 3）vs 顺子长度 6（起始 3）：长度不同不可压
        let straight5 = val(6, 3, vec![5]);
        let straight6 = val(6, 3, vec![6]);
        assert_eq!(cmp.compare(&straight6, &straight5), Ordering::Less);
        // 同长度比起始点数
        let straight5_from4 = val(6, 4, vec![5]);
        assert_eq!(cmp.compare(&straight5_from4, &straight5), Ordering::Greater);
    }

    #[test]
    fn bomb_vs_bomb_compares_primary() {
        let cmp = DouDizhuComparator::new();
        let bomb9 = val(13, 9, vec![]);
        let bomb_ace = val(13, 14, vec![]);
        assert_eq!(cmp.compare(&bomb_ace, &bomb9), Ordering::Greater);
        assert_eq!(cmp.compare(&bomb9, &bomb_ace), Ordering::Less);
    }
}
