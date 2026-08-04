//! 斗地主预设完整使用示例：
//! 配置 → 发牌 → 叫地主 → 分配底牌 → 注册监听 → 出牌驱动 → 终局结算。

use boardgame::engine::{Action, GameEngine};
use boardgame::presets::dou_dizhu::bidding::BidAction;
use boardgame::presets::dou_dizhu::effects::{
    BombMultiplierListener, RoundEndListener, SettlementListener, SpringListener,
};
use boardgame::presets::dou_dizhu::scoring::ScoreBoard;
use boardgame::presets::dou_dizhu::setup::{DouDizhuConfig, DouDizhuSetup};
use boardgame::presets::dou_dizhu::validator::DouDizhuValidator;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::{Arc, Mutex};

fn main() {
    let config = DouDizhuConfig::default();
    let mut rng = StdRng::seed_from_u64(2024);
    let player_ids = [1u32, 2, 3];

    // 1. 发牌
    let (mut session, mut state) = DouDizhuSetup::new_game(&config, player_ids, &mut rng);
    println!(
        "发牌完成：各玩家 {} 张，底牌 {} 张",
        config.deal_per_player, config.bottom_cards
    );

    // 2. 叫地主（演示：玩家 1 直接叫 3 分）
    session
        .submit(1, BidAction::Bid(3))
        .expect("bid should succeed");
    let outcome = session.outcome().expect("叫地主应已完成");
    let landlord = outcome.landlord;
    let bid_score = outcome.bid_score;
    println!("地主：玩家 {landlord}，叫分 {bid_score}");

    // 3. 分配底牌
    DouDizhuSetup::assign_bottom(&mut state, landlord);
    println!(
        "地主获得底牌，手牌 {} 张",
        state
            .players_hands
            .get(&landlord)
            .expect("landlord exists")
            .len()
    );

    // 4. 初始化计分板与监听器
    let scoreboard = Arc::new(Mutex::new(ScoreBoard::new(bid_score)));
    let mut farmers = player_ids.iter().copied().filter(|&p| p != landlord);
    let farmers_arr = [
        farmers.next().expect("farmer 1"),
        farmers.next().expect("farmer 2"),
    ];

    let mut engine = GameEngine::new(DouDizhuValidator::new(), state);
    engine.register_listener(RoundEndListener::default());
    engine.register_listener(BombMultiplierListener::new(
        scoreboard.clone(),
        config.bomb_multiplier,
        config.rocket_multiplier,
    ));
    engine.register_listener(SpringListener::new(
        scoreboard.clone(),
        landlord,
        farmers_arr,
        config.enable_spring,
        config.enable_anti_spring,
        config.spring_multiplier,
    ));
    engine.register_listener(SettlementListener::new(
        scoreboard.clone(),
        landlord,
        farmers_arr,
    ));

    // 5. 演示出牌若干轮（简化：每轮取最小一张打出，失败则 Pass）
    let mut current = landlord;
    for round in 0..3 {
        engine.state.current_player = current;

        let hand_empty = engine
            .state
            .players_hands
            .get(&current)
            .is_none_or(|h| h.is_empty());
        if hand_empty {
            break;
        }

        let (card_id, card_rank) = {
            let hand = engine
                .state
                .players_hands
                .get(&current)
                .expect("hand exists");
            let min_card = hand
                .iter()
                .iter()
                .min_by_key(|c| c.rank)
                .expect("hand non-empty");
            (min_card.id, min_card.rank)
        };

        match engine.transition(Action::Play(current, vec![card_id])) {
            Ok(()) => {
                let remaining = engine
                    .state
                    .players_hands
                    .get(&current)
                    .map_or(0, |h| h.len());
                println!(
                    "[轮 {round}] 玩家 {current} 出牌 {:?}，剩余手牌 {remaining} 张",
                    card_rank
                );
            }
            Err(e) => {
                println!("[轮 {round}] 玩家 {current} 出牌失败：{e:?}，改为 Pass");
                let _ = engine.transition(Action::Pass(current));
            }
        }

        current = next_player(current, player_ids);
    }

    // 6. 结算信息
    let sb = scoreboard.lock().expect("scoreboard lock");
    println!(
        "最终倍数：{}，最终分：{}",
        sb.multiplier(),
        sb.final_score()
    );
    println!("各玩家分数：{:?}", engine.state.player_states.scores);
    println!("斗地主示例运行完成！");
}

fn next_player(current: u32, all: [u32; 3]) -> u32 {
    let idx = all
        .iter()
        .position(|&p| p == current)
        .expect("current in all");
    all[(idx + 1) % 3]
}
