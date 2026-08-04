//! 斗地主叫地主阶段。
//!
//! 设计要点：
//! - [`BiddingSession`] 独立管理叫分流程，不依赖 [`GameEngine`](crate::engine::GameEngine)，
//!   定地主后由调用方将底牌交给地主并构造 `GameState`
//! - 支持 `Classic`（叫分 1..=max）与 `Simple`（叫/不叫）两种制式
//! - 叫分制下叫 max 分直接定地主

use std::fmt::Debug;

/// 叫分制式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiddingMode {
    /// 经典叫分：玩家可叫 1..=max_score 分或不叫；叫 max_score 直接定地主。
    Classic { max_score: u32 },
    /// 简化叫地主：玩家选择叫或不叫；首叫者为地主。
    Simple,
}

impl Default for BiddingMode {
    fn default() -> Self {
        BiddingMode::Classic { max_score: 3 }
    }
}

/// 叫分动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidAction {
    /// 不叫
    Pass,
    /// 叫分（Classic 模式下为 1..=max_score；Simple 模式忽略数值）
    Bid(u32),
}

/// 叫地主结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiddingOutcome {
    /// 地主玩家 id
    pub landlord: u32,
    /// 最终叫分（基础分）
    pub bid_score: u32,
    /// 底牌的 card id 列表（用于将底牌交给地主）
    pub bottom_card_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiddingError {
    /// 非该玩家轮次
    NotYourTurn,
    /// 叫分超出上限或非法
    InvalidBid,
    /// 流程已结束
    AlreadyFinished,
}

/// 叫地主会话状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiddingSession {
    mode: BiddingMode,
    /// 按出牌顺序排列的玩家 id
    player_ids: [u32; 3],
    /// 底牌 id 列表
    bottom_card_ids: Vec<u32>,
    /// 当前轮次索引（0..3）
    current_idx: usize,
    /// 已提交的动作序列（player_id, action）
    history: Vec<(u32, BidAction)>,
    /// 当前最高叫分与叫家
    highest_bid: Option<(u32, u32)>,
    /// 是否已结束
    finished: bool,
    /// 结束时的结果（仅 finished=true 时有值）
    outcome: Option<BiddingOutcome>,
}

impl BiddingSession {
    /// 创建新的叫地主会话。
    ///
    /// `player_ids` 按叫分顺序排列（首位为首叫）。
    pub fn new(mode: BiddingMode, player_ids: [u32; 3], bottom_card_ids: Vec<u32>) -> Self {
        Self {
            mode,
            player_ids,
            bottom_card_ids,
            current_idx: 0,
            history: Vec::new(),
            highest_bid: None,
            finished: false,
            outcome: None,
        }
    }

    /// 当前应叫玩家 id。
    pub fn current_player(&self) -> u32 {
        self.player_ids[self.current_idx]
    }

    /// 提交一次叫分动作。
    pub fn submit(&mut self, player_id: u32, action: BidAction) -> Result<(), BiddingError> {
        if self.finished {
            return Err(BiddingError::AlreadyFinished);
        }
        if player_id != self.current_player() {
            return Err(BiddingError::NotYourTurn);
        }
        // 校验叫分
        match (self.mode, action) {
            (BiddingMode::Classic { max_score }, BidAction::Bid(score)) => {
                if score < 1 || score > max_score {
                    return Err(BiddingError::InvalidBid);
                }
                // 必须严格大于当前最高叫分（若存在）
                if let Some((_, cur)) = self.highest_bid
                    && score <= cur
                {
                    return Err(BiddingError::InvalidBid);
                }
            }
            (BiddingMode::Simple, BidAction::Bid(_)) => {
                // Simple 模式忽略数值，仅表示「叫」
            }
            (_, BidAction::Pass) => {}
        }

        // 记录历史
        self.history.push((player_id, action));

        // 更新最高叫分
        match action {
            BidAction::Bid(score) => {
                self.highest_bid = Some((player_id, score));
                // Classic 模式叫 max_score 直接定地主
                if let BiddingMode::Classic { max_score } = self.mode
                    && score >= max_score
                {
                    self.finish(player_id, score);
                    return Ok(());
                }
                // Simple 模式首叫即定地主
                if matches!(self.mode, BiddingMode::Simple) {
                    self.finish(player_id, 1);
                    return Ok(());
                }
            }
            BidAction::Pass => {}
        }

        // 推进轮次
        self.current_idx += 1;
        // 判断是否全员结束
        if self.current_idx >= self.player_ids.len() {
            // 所有人都叫过一轮
            if let Some((landlord, score)) = self.highest_bid {
                self.finish(landlord, score);
                return Ok(());
            } else {
                // 全员 Pass
                self.finished = true;
                self.outcome = None;
                return Ok(());
            }
        }
        Ok(())
    }

    fn finish(&mut self, landlord: u32, bid_score: u32) {
        self.finished = true;
        self.outcome = Some(BiddingOutcome {
            landlord,
            bid_score,
            bottom_card_ids: self.bottom_card_ids.clone(),
        });
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// 结束时返回结果；全员 Pass 返回 `None`。
    pub fn outcome(&self) -> Option<&BiddingOutcome> {
        self.outcome.as_ref()
    }

    /// 已提交的动作历史。
    pub fn history(&self) -> &[(u32, BidAction)] {
        &self.history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_bid_max_score_finishes_immediately() {
        let mut s = BiddingSession::new(
            BiddingMode::Classic { max_score: 3 },
            [1, 2, 3],
            vec![100, 101, 102],
        );
        s.submit(1, BidAction::Bid(3)).unwrap();
        assert!(s.is_finished());
        let o = s.outcome().unwrap();
        assert_eq!(o.landlord, 1);
        assert_eq!(o.bid_score, 3);
        assert_eq!(o.bottom_card_ids, vec![100, 101, 102]);
    }

    #[test]
    fn classic_highest_bid_wins() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        s.submit(1, BidAction::Bid(1)).unwrap();
        assert!(!s.is_finished());
        s.submit(2, BidAction::Bid(2)).unwrap();
        s.submit(3, BidAction::Pass).unwrap();
        assert!(s.is_finished());
        let o = s.outcome().unwrap();
        assert_eq!(o.landlord, 2);
        assert_eq!(o.bid_score, 2);
    }

    #[test]
    fn classic_all_pass_returns_none_outcome() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        s.submit(1, BidAction::Pass).unwrap();
        s.submit(2, BidAction::Pass).unwrap();
        s.submit(3, BidAction::Pass).unwrap();
        assert!(s.is_finished());
        assert!(s.outcome().is_none());
    }

    #[test]
    fn classic_bid_below_or_equal_current_rejected() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        s.submit(1, BidAction::Bid(2)).unwrap();
        // 玩家 2 叫 2 不大于 2 → 拒绝
        let err = s.submit(2, BidAction::Bid(2)).unwrap_err();
        assert_eq!(err, BiddingError::InvalidBid);
        // 叫 0 或 >3 也拒绝
        s.submit(2, BidAction::Bid(3)).unwrap(); // 3 > 2 通过
    }

    #[test]
    fn classic_bid_out_of_range_rejected() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        assert_eq!(
            s.submit(1, BidAction::Bid(0)).unwrap_err(),
            BiddingError::InvalidBid
        );
        assert_eq!(
            s.submit(1, BidAction::Bid(4)).unwrap_err(),
            BiddingError::InvalidBid
        );
    }

    #[test]
    fn not_your_turn_rejected() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        assert_eq!(
            s.submit(2, BidAction::Bid(1)).unwrap_err(),
            BiddingError::NotYourTurn
        );
    }

    #[test]
    fn submit_after_finished_rejected() {
        let mut s = BiddingSession::new(BiddingMode::Classic { max_score: 3 }, [1, 2, 3], vec![]);
        s.submit(1, BidAction::Bid(3)).unwrap();
        assert_eq!(
            s.submit(2, BidAction::Pass).unwrap_err(),
            BiddingError::AlreadyFinished
        );
    }

    #[test]
    fn simple_mode_first_bidder_wins() {
        let mut s = BiddingSession::new(BiddingMode::Simple, [1, 2, 3], vec![]);
        s.submit(1, BidAction::Pass).unwrap();
        s.submit(2, BidAction::Bid(1)).unwrap(); // Simple 忽略数值
        assert!(s.is_finished());
        let o = s.outcome().unwrap();
        assert_eq!(o.landlord, 2);
        assert_eq!(o.bid_score, 1);
    }
}
