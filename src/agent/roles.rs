use std::collections::HashSet;
use crate::agent::prompt::AgentRole;
use regex::Regex;

/// Get the set of tool names allowed for a role
pub fn get_tools_for_role(role: &AgentRole, goal: &str) -> Vec<String> {
    match role {
        AgentRole::Manager => MANAGER_TOOLS.iter().map(|s| s.to_string()).collect(),
        AgentRole::Screener => SCREENER_TOOLS.iter().map(|s| s.to_string()).collect(),
        AgentRole::General => get_general_tools(goal),
    }
}

static MANAGER_TOOLS: &[&str] = &[
    "close_position", "claim_fees", "swap_token",
    "get_position_pnl", "get_my_positions", "get_wallet_balance",
];

static SCREENER_TOOLS: &[&str] = &[
    "deploy_position", "get_active_bin", "get_top_candidates",
    "check_smart_wallets_on_pool", "get_token_holders", "get_token_narrative",
    "get_token_info", "search_pools", "get_pool_memory", "get_wallet_balance",
    "get_my_positions",
];

struct IntentPattern {
    intent: &'static str,
    tools: &'static [&'static str],
    pattern: &'static str,
}

static INTENT_MAP: &[IntentPattern] = &[
    IntentPattern { intent: "deploy", tools: &["deploy_position", "get_top_candidates", "get_active_bin", "get_pool_memory", "check_smart_wallets_on_pool", "get_token_holders", "get_token_narrative", "get_token_info", "search_pools", "get_wallet_balance", "get_my_positions", "add_pool_note"], pattern: r"(?i)(deploy|open|add liquidity|lp into|invest in)" },
    IntentPattern { intent: "close", tools: &["close_position", "get_my_positions", "get_position_pnl", "get_wallet_balance", "swap_token"], pattern: r"(?i)(close|exit|withdraw|remove liquidity|shut down)" },
    IntentPattern { intent: "claim", tools: &["claim_fees", "get_my_positions", "get_position_pnl", "get_wallet_balance"], pattern: r"(?i)(claim|harvest|collect).*fee" },
    IntentPattern { intent: "swap", tools: &["swap_token", "get_wallet_balance"], pattern: r"(?i)(swap|convert|sell|exchange)" },
    IntentPattern { intent: "balance", tools: &["get_wallet_balance", "get_my_positions"], pattern: r"(?i)(balance|wallet|sol|how much)" },
    IntentPattern { intent: "positions", tools: &["get_my_positions", "get_position_pnl", "get_wallet_balance"], pattern: r"(?i)(position|portfolio|open|pnl|yield|range)" },
    IntentPattern { intent: "screen", tools: &["get_top_candidates", "get_token_holders", "get_token_narrative", "get_token_info", "search_pools", "check_smart_wallets_on_pool", "get_my_positions", "discover_pools"], pattern: r"(?i)(screen|candidate|find pool|search|research|token)" },
    IntentPattern { intent: "memory", tools: &["get_pool_memory", "add_pool_note"], pattern: r"(?i)(memory|pool history|note|remember)" },
];

fn get_general_tools(goal: &str) -> Vec<String> {
    let mut matched = HashSet::new();

    for ip in INTENT_MAP {
        if let Ok(re) = Regex::new(ip.pattern) {
            if re.is_match(goal) {
                for t in ip.tools { matched.insert(t.to_string()); }
            }
        }
    }

    if matched.is_empty() {
        // Fallback: all non-destructive tools
        return vec![
            "get_wallet_balance", "get_my_positions", "get_position_pnl",
            "get_top_candidates", "search_pools", "get_token_info",
            "get_token_holders", "get_token_narrative", "get_pool_memory",
            "check_smart_wallets_on_pool", "get_active_bin", "discover_pools",
        ].into_iter().map(String::from).collect();
    }

    matched.into_iter().collect()
}
