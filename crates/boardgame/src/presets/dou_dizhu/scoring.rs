//! 斗地主计分与倍数跟踪。
//!
//! 设计要点：
//! - [`ScoreBoard`] 跟踪当前倍数与基础分，支持炸弹/王炸/春天/反春天翻倍
//! - [`GameOutcome`] 描述终局胜负与最终分数分配
//! - 计分规则：final = base × multiplier；地主胜地主 +2×final，两农民各 −final；农民胜反之

use std::collections::HashMap;

/// 胜方阵营。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Landlord,
    Farmer,
}

/// 终局结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOutcome {
    pub winner_side: Side,
    /// 各玩家最终得分（地主分/失 2×final，农民各分/失 final）
    pub final_scores: HashMap<u32, i32>,
}

/// 计分板：跟踪基础分与累计倍数。
///
/// 通过 `Arc<Mutex<ScoreBoard>>` 在多个 `EventListener` 间共享。
#[derive(Debug, Clone)]
pub struct ScoreBoard {
    base_score: u32,
    multiplier: u32,
    spring_applied: bool,
    anti_spring_applied: bool,
}

impl ScoreBoard {
    pub fn new(base_score: u32) -> Self {
        Self {
            base_score,
            multiplier: 1,
            spring_applied: false,
            anti_spring_applied: false,
        }
    }

    /// 当前基础分。
    pub fn base_score(&self) -> u32 {
        self.base_score
    }

    /// 当前累计倍数。
    pub fn multiplier(&self) -> u32 {
        self.multiplier
    }

    /// 倍数 ×m。
    pub fn multiply(&mut self, m: u32) {
        self.multiplier = self.multiplier.saturating_mul(m);
    }

    /// 应用春天翻倍（仅生效一次）。
    pub fn apply_spring(&mut self, spring_multiplier: u32) {
        if !self.spring_applied {
            self.multiply(spring_multiplier);
            self.spring_applied = true;
        }
    }

    /// 应用反春天翻倍（仅生效一次）。
    pub fn apply_anti_spring(&mut self, spring_multiplier: u32) {
        if !self.anti_spring_applied {
            self.multiply(spring_multiplier);
            self.anti_spring_applied = true;
        }
    }

    /// 当前最终分（= base × multiplier）。
    pub fn final_score(&self) -> u32 {
        self.base_score.saturating_mul(self.multiplier)
    }

    /// 结算：根据胜方与玩家身份计算最终分数分配。
    ///
    /// - 地主胜：地主 +2×final，两农民各 −final
    /// - 农民胜：地主 −2×final，两农民各 +final
    pub fn settle(&self, winner_side: Side, landlord_id: u32, farmer_ids: [u32; 2]) -> GameOutcome {
        let final_score = self.final_score() as i32;
        let mut final_scores = HashMap::new();
        let landlord_delta = 2 * final_score;
        let farmer_delta = final_score;
        match winner_side {
            Side::Landlord => {
                final_scores.insert(landlord_id, landlord_delta);
                for &f in &farmer_ids {
                    final_scores.insert(f, -farmer_delta);
                }
            }
            Side::Farmer => {
                final_scores.insert(landlord_id, -landlord_delta);
                for &f in &farmer_ids {
                    final_scores.insert(f, farmer_delta);
                }
            }
        }
        GameOutcome {
            winner_side,
            final_scores,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bomb_multipliers_stack() {
        let mut sb = ScoreBoard::new(3);
        sb.multiply(2); // 一个炸弹
        sb.multiply(2); // 第二个炸弹
        assert_eq!(sb.multiplier(), 4);
        assert_eq!(sb.final_score(), 12); // 3 × 4
    }

    #[test]
    fn spring_doubles_once() {
        let mut sb = ScoreBoard::new(1);
        sb.apply_spring(2);
        sb.apply_spring(2); // 重复应用不生效
        assert_eq!(sb.multiplier(), 2);
        assert_eq!(sb.final_score(), 2);
    }

    #[test]
    fn landlord_win_distribution() {
        let mut sb = ScoreBoard::new(3);
        sb.multiply(2); // 倍数 2（一个炸弹）
        let outcome = sb.settle(Side::Landlord, 1, [2, 3]);
        // final = 3 × 2 = 6；地主 +12，农民各 -6
        assert_eq!(outcome.final_scores.get(&1), Some(&12));
        assert_eq!(outcome.final_scores.get(&2), Some(&-6));
        assert_eq!(outcome.final_scores.get(&3), Some(&-6));
    }

    #[test]
    fn farmer_win_distribution() {
        let sb = ScoreBoard::new(2);
        let outcome = sb.settle(Side::Farmer, 1, [2, 3]);
        // final = 2；地主 -4，农民各 +2
        assert_eq!(outcome.final_scores.get(&1), Some(&-4));
        assert_eq!(outcome.final_scores.get(&2), Some(&2));
        assert_eq!(outcome.final_scores.get(&3), Some(&2));
    }

    #[test]
    fn spring_then_bomb_stacks() {
        let mut sb = ScoreBoard::new(1);
        sb.apply_spring(2); // ×2
        sb.multiply(2); // 炸弹 ×2
        assert_eq!(sb.multiplier(), 4);
        assert_eq!(sb.final_score(), 4);
    }

    #[test]
    fn anti_spring_separate_from_spring() {
        let mut sb = ScoreBoard::new(1);
        sb.apply_spring(2);
        sb.apply_anti_spring(2);
        assert_eq!(sb.multiplier(), 4);
    }
}
