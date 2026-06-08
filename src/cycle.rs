use anyhow::Result;
use crate::agent::loop_::AgentLoop;
use crate::agent::prompt::AgentRole;
use crate::config::Config;
use crate::config::loader::compute_deploy_amount;
use crate::llm::LlmClient;
use crate::state::positions::PositionState;
use crate::state::pool_memory::PoolMemoryStore;
use crate::utils::logger::module::info;

pub async fn run_management_cycle(
    config: &Config,
    llm: &LlmClient,
    positions: &PositionState,
    pool_memory: &PoolMemoryStore,
) -> Result<String> {
    info("cycle", "Management Cycle Starting");

    let active = positions.get_active();
    if active.is_empty() {
        info("cycle", "No active positions to manage.");
        return Ok("No active positions.".to_string());
    }

    let pos_json = serde_json::to_string(&active).unwrap_or_else(|_| "[]".to_string());
    let goal = format!(
        "Evaluate all open positions. Apply TP/SL/OOR rules. Current positions:
{}

Management config: OOR wait = {} min, TP = {:?}%",
        pos_json,
        config.management.out_of_range_wait_minutes,
        config.management.take_profit_pct,
    );

    let agent = AgentLoop::new();
    let result = agent.run(&goal, AgentRole::Manager, config, llm, positions, pool_memory).await?;

    info("cycle", &format!("Management result: {}", &result[..result.len().min(300)]));
    Ok(result)
}

pub async fn run_screening_cycle(
    config: &Config,
    llm: &LlmClient,
    positions: &PositionState,
    pool_memory: &PoolMemoryStore,
    wallet_sol: f64,
) -> Result<String> {
    info("cycle", "Screening Cycle Starting");

    let active_count = positions.count_active();
    if active_count >= config.risk.max_positions as usize {
        let msg = format!("At max positions ({}/{}). Skipping.", active_count, config.risk.max_positions);
        info("cycle", &msg);
        return Ok(msg);
    }

    let deploy_amount = compute_deploy_amount(config, wallet_sol);
    if deploy_amount <= 0.0 {
        let msg = format!("Not enough SOL ({:.2}) to deploy.", wallet_sol);
        info("cycle", &msg);
        return Ok(msg);
    }

    let goal = format!(
        "Screen Meteora DLMM pools and deploy {:.4} SOL to the best candidate.          Active positions: {}. Max: {}.          Use get_top_candidates, then call deploy_position with amount_y={:.4}.",
        deploy_amount, active_count, config.risk.max_positions, deploy_amount,
    );

    let agent = AgentLoop::new();
    let result = agent.run(&goal, AgentRole::Screener, config, llm, positions, pool_memory).await?;

    info("cycle", &format!("Screening result: {}", &result[..result.len().min(300)]));
    Ok(result)
}
