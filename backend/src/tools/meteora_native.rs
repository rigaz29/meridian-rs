use anyhow::{anyhow, Result};
use solana_client_v3::nonblocking::rpc_client::RpcClient;
use solana_sdk_v3::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::{str::FromStr, sync::Arc};
use wp_solana_core::token::WorkspacePlanConfig;
use wp_solana_meteora_dlmm_client::generated::accounts::LbPair;
use wp_solana_meteora_dlmm_client::generated::types::{StrategyParameters, StrategyType};
use wp_solana_meteora_dlmm_core::plan::{
    add_liquidity::{AddLiquidityParams, NewPositionConfig},
    claim_fee::ClaimFeeParams,
    close_position::ClosePositionParams,
};
use wp_solana_meteora_dlmm_sdk::orchestrate::{
    add_liquidity::add_liquidity_one_shot, claim_fee::claim_fee_one_shot,
    close_position::close_position_one_shot,
};
use wp_solana_rpc::RpcContext;

use crate::config::Config;

const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeClaimRequest {
    pub position_address: String,
    pub authority: String,
    pub rpc_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCloseRequest {
    pub position_address: String,
    pub authority: String,
    pub rent_receiver: Option<String>,
    pub rpc_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDeployRequest {
    pub pool_address: String,
    pub position_address: String,
    pub authority: String,
    pub rpc_url: String,
    pub amount_x: u64,
    pub amount_y: u64,
    pub active_id: i32,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub width: i32,
    pub strategy: String,
    pub max_active_bin_slippage: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeClaimResult {
    pub signature: String,
    pub claimable_fee_x: u64,
    pub claimable_fee_y: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCloseResult {
    pub signature: String,
    /// Signature of the follow-up tx that unwrapped wSOL back to native SOL, if
    /// any wSOL was present after the close. `None` when nothing needed sweeping.
    pub unwrap_signature: Option<String>,
    pub remove_liquidity_amount_x: u64,
    pub remove_liquidity_amount_y: u64,
    pub claimable_fee_x: u64,
    pub claimable_fee_y: u64,
    pub claimable_rewards: [u64; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDeployResult {
    pub signature: String,
    pub position_address: String,
}

pub fn keypair_from_secret(secret: &str) -> Result<Keypair> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        anyhow::bail!("wallet private key is empty");
    }
    // Accept a Solana CLI keypair FILE PATH (e.g. id.json) in addition to a raw
    // base58 / JSON-array secret. Use forward slashes on Windows so the path
    // survives .env parsing.
    let source = if !trimmed.starts_with('[') && std::path::Path::new(trimmed).is_file() {
        std::fs::read_to_string(trimmed)
            .map_err(|e| anyhow!("failed to read keypair file {}: {}", trimmed, e))?
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    let source = source.as_str();
    let bytes = if source.starts_with('[') {
        serde_json::from_str::<Vec<u8>>(source)
            .map_err(|e| anyhow!("invalid JSON wallet private key: {}", e))?
    } else {
        bs58::decode(source)
            .into_vec()
            .map_err(|e| anyhow!("invalid base58 wallet private key: {}", e))?
    };
    if bytes.len() != 64 {
        anyhow::bail!(
            "wallet private key must decode to 64 bytes, got {} bytes",
            bytes.len()
        );
    }
    Keypair::try_from(bytes.as_slice()).map_err(|e| anyhow!("invalid wallet private key: {}", e))
}

pub fn keypair_pubkey_from_secret(secret: &str) -> Result<String> {
    Ok(keypair_from_secret(secret)?.pubkey().to_string())
}

pub fn wallet_secret_from_env() -> Result<String> {
    ["WALLET_PRIVATE_KEY", "MERIDIAN_WALLET_PRIVATE_KEY"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
        .ok_or_else(|| anyhow!("WALLET_PRIVATE_KEY or MERIDIAN_WALLET_PRIVATE_KEY is required for native Meteora transactions"))
}

pub fn resolve_rpc_url(config: &Config) -> String {
    config
        .api
        .helius_rpc_url
        .clone()
        .or_else(|| std::env::var("HELIUS_RPC_URL").ok())
        .or_else(|| std::env::var("RPC_URL").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RPC_URL.to_string())
}

fn parse_pubkey(label: &str, value: &str) -> Result<Pubkey> {
    Pubkey::from_str(value).map_err(|e| anyhow!("invalid {} {}: {}", label, value, e))
}

fn sol_to_lamports(amount_sol: f64) -> Result<u64> {
    if !amount_sol.is_finite() || amount_sol <= 0.0 {
        anyhow::bail!("amount_sol must be a positive finite number");
    }
    Ok((amount_sol * 1_000_000_000.0).floor() as u64)
}

fn strategy_type_from_name(strategy: &str) -> StrategyType {
    // Single-side SOL deposits (amount_x = 0) are IMBALANCED, so AddLiquidityByStrategy2
    // needs the *ImBalanced strategy type. *Balanced requires both tokens in equal
    // value (deposits ~0 when one side is empty, leaving wrapped SOL stuck); *OneSide
    // belongs to a different instruction (InvalidStrategyParameters / 0x17a6). The
    // original JS passes TS-SDK `StrategyType.Spot`, which maps to SpotImBalanced for
    // single-side deposits.
    match strategy.to_ascii_lowercase().replace('-', "_").as_str() {
        "curve" | "curve_one_side" | "curve_balanced" | "curve_imbalanced" => {
            StrategyType::CurveImBalanced
        }
        "bid_ask" | "bidask" | "bid_ask_one_side" | "bid_ask_balanced" | "bid_ask_imbalanced" => {
            StrategyType::BidAskImBalanced
        }
        _ => StrategyType::SpotImBalanced,
    }
}

fn strategy_name(strategy_type: StrategyType) -> &'static str {
    match strategy_type {
        StrategyType::CurveImBalanced => "curve_imbalanced",
        StrategyType::BidAskImBalanced => "bid_ask_imbalanced",
        _ => "spot_imbalanced",
    }
}

fn strategy_parameters(min_bin_id: i32, max_bin_id: i32, strategy: &str) -> StrategyParameters {
    StrategyParameters {
        min_bin_id,
        max_bin_id,
        strategy_type: strategy_type_from_name(strategy),
        parameteres: [0; 64],
    }
}

fn bin_range(active_id: i32, bins_below: i64, bins_above: i64) -> Result<(i32, i32, i32)> {
    if bins_below < 0 || bins_above < 0 {
        anyhow::bail!("bins_below and bins_above must be non-negative");
    }
    let min_bin_id = active_id
        .checked_sub(bins_below as i32)
        .ok_or_else(|| anyhow!("min bin underflow"))?;
    let max_bin_id = active_id
        .checked_add(bins_above as i32)
        .ok_or_else(|| anyhow!("max bin overflow"))?;
    let width = max_bin_id
        .checked_sub(min_bin_id)
        .and_then(|span| span.checked_add(1))
        .ok_or_else(|| anyhow!("invalid bin range"))?;
    if width <= 0 {
        anyhow::bail!("bin range width must be positive");
    }
    Ok((min_bin_id, max_bin_id, width))
}

#[derive(Debug, Clone, Copy)]
pub struct NativeDeployBuildInput<'a> {
    pub pool_address: &'a str,
    pub amount_sol: f64,
    pub active_id: i32,
    pub bins_below: i64,
    pub bins_above: i64,
    pub strategy: &'a str,
}

pub fn build_deploy_request(
    input: NativeDeployBuildInput<'_>,
    config: &Config,
    wallet_secret: &str,
) -> Result<NativeDeployRequest> {
    let keypair = keypair_from_secret(wallet_secret)?;
    let pool = parse_pubkey("DLMM pool address", input.pool_address)?;
    let position_keypair = Keypair::new();
    let (min_bin_id, max_bin_id, width) =
        bin_range(input.active_id, input.bins_below, input.bins_above)?;
    let strategy_type = strategy_type_from_name(input.strategy);

    Ok(NativeDeployRequest {
        pool_address: pool.to_string(),
        position_address: position_keypair.pubkey().to_string(),
        authority: keypair.pubkey().to_string(),
        rpc_url: resolve_rpc_url(config),
        amount_x: 0,
        amount_y: sol_to_lamports(input.amount_sol)?,
        active_id: input.active_id,
        min_bin_id,
        max_bin_id,
        width,
        strategy: strategy_name(strategy_type).to_string(),
        max_active_bin_slippage: 1,
    })
}

pub fn build_claim_request(
    position_address: &str,
    config: &Config,
    wallet_secret: &str,
) -> Result<NativeClaimRequest> {
    let keypair = keypair_from_secret(wallet_secret)?;
    let position = parse_pubkey("DLMM position address", position_address)?;

    Ok(NativeClaimRequest {
        position_address: position.to_string(),
        authority: keypair.pubkey().to_string(),
        rpc_url: resolve_rpc_url(config),
    })
}

pub fn build_close_request(
    position_address: &str,
    config: &Config,
    wallet_secret: &str,
    rent_receiver: Option<&str>,
) -> Result<NativeCloseRequest> {
    let keypair = keypair_from_secret(wallet_secret)?;
    let position = parse_pubkey("DLMM position address", position_address)?;
    let authority = keypair.pubkey().to_string();
    let rent_receiver = rent_receiver
        .map(|value| parse_pubkey("rent receiver", value).map(|pubkey| pubkey.to_string()))
        .transpose()?
        .or_else(|| Some(authority.clone()));

    Ok(NativeCloseRequest {
        position_address: position.to_string(),
        authority,
        rent_receiver,
        rpc_url: resolve_rpc_url(config),
    })
}

pub async fn deploy_position(
    pool_address: &str,
    amount_sol: f64,
    active_id: i32,
    bins_below: i64,
    bins_above: i64,
    strategy: &str,
    config: &Config,
) -> Result<NativeDeployResult> {
    let wallet_secret = wallet_secret_from_env()?;
    let keypair = keypair_from_secret(&wallet_secret)?;
    let pool = parse_pubkey("DLMM pool address", pool_address)?;
    let amount_y = sol_to_lamports(amount_sol)?;

    let rpc_url = resolve_rpc_url(config);
    let rpc_client = RpcClient::new(rpc_url);

    // The Meteora pool-discovery API does not expose the active bin id, so the
    // caller's `active_id` is often a placeholder (0). Read the authoritative
    // active_id straight from the on-chain LbPair account so the strategy bins
    // and the bin-slippage check match the real pool state — otherwise the tx
    // is rejected with ExceededBinSlippageTolerance (0x1774).
    let active_id = match rpc_client.get_account_data(&pool).await {
        Ok(data) => match LbPair::from_bytes(&data) {
            Ok(lb_pair) => lb_pair.active_id,
            Err(e) => {
                tracing::warn!("failed to decode LbPair {}: {}; using caller active_id", pool, e);
                active_id
            }
        },
        Err(e) => {
            tracing::warn!("failed to fetch LbPair {}: {}; using caller active_id", pool, e);
            active_id
        }
    };

    let (min_bin_id, max_bin_id, width) = bin_range(active_id, bins_below, bins_above)?;
    let position_keypair = Keypair::new();
    let position_address = position_keypair.pubkey();
    let params = AddLiquidityParams {
        lb_pair_address: pool,
        position_address,
        new_position: Some(NewPositionConfig {
            position_keypair,
            lower_bin_id: min_bin_id,
            width,
        }),
        amount_x: 0,
        amount_y,
        active_id,
        // Tolerate a few bins of movement between fetch and execution.
        max_active_bin_slippage: 5,
        strategy_parameters: strategy_parameters(min_bin_id, max_bin_id, strategy),
        authority: keypair.pubkey(),
    };

    let rpc_ctx = RpcContext::confirmed(Arc::new(rpc_client));
    let plan_config = WorkspacePlanConfig::default();
    let result = add_liquidity_one_shot(&rpc_ctx, params, &plan_config, &keypair)
        .await
        .map_err(|e| anyhow!("native Meteora add_liquidity_one_shot failed: {}", e))?;

    Ok(NativeDeployResult {
        signature: result.signature.to_string(),
        position_address: result.position_address.to_string(),
    })
}

pub async fn claim_fees(position_address: &str, config: &Config) -> Result<NativeClaimResult> {
    let wallet_secret = wallet_secret_from_env()?;
    let keypair = keypair_from_secret(&wallet_secret)?;
    let position = parse_pubkey("DLMM position address", position_address)?;
    let rpc_url = resolve_rpc_url(config);
    let rpc_client = RpcClient::new(rpc_url);
    let rpc_ctx = RpcContext::confirmed(Arc::new(rpc_client));
    let params = ClaimFeeParams {
        position_address: position,
        authority: keypair.pubkey(),
    };
    let plan_config = WorkspacePlanConfig::default();
    let result = claim_fee_one_shot(&rpc_ctx, params, &plan_config, &keypair)
        .await
        .map_err(|e| anyhow!("native Meteora claim_fee_one_shot failed: {}", e))?;

    Ok(NativeClaimResult {
        signature: result.signature.to_string(),
        claimable_fee_x: result.quote.claimable_fee_x,
        claimable_fee_y: result.quote.claimable_fee_y,
    })
}

/// SPL Token program id.
const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
/// Native mint (wrapped SOL).
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
/// SPL Token `CloseAccount` instruction discriminator.
const SPL_TOKEN_CLOSE_ACCOUNT_IX: u8 = 9;

/// Close every wSOL token account owned by `keypair`, unwrapping the balance
/// (wrapped principal + account rent) back to native SOL on the same wallet.
///
/// The `CloseAccount` instruction is built by hand (program id + 3 accounts +
/// a single discriminator byte) to avoid pulling an spl-token crate whose
/// solana types would clash with the v3 transaction stack used here.
///
/// Returns the sweep tx signature, or `None` when the wallet holds no wSOL.
/// Build an SPL Token `CloseAccount` instruction by hand. Closing a wSOL
/// account transfers its lamports (wrapped principal + rent) to `destination`,
/// which is how an unwrap is performed.
fn close_wsol_account_ix(
    token_program: Pubkey,
    account: Pubkey,
    owner: Pubkey,
) -> solana_sdk_v3::instruction::Instruction {
    use solana_sdk_v3::instruction::{AccountMeta, Instruction};
    Instruction {
        program_id: token_program,
        accounts: vec![
            AccountMeta::new(account, false), // wSOL account to close
            AccountMeta::new(owner, false),   // lamports destination
            AccountMeta::new_readonly(owner, true), // account owner (signer)
        ],
        data: vec![SPL_TOKEN_CLOSE_ACCOUNT_IX],
    }
}

async fn unwrap_wsol(rpc: &RpcClient, keypair: &Keypair) -> Result<Option<String>> {
    use solana_client_v3::rpc_request::TokenAccountsFilter;
    use solana_sdk_v3::instruction::Instruction;
    use solana_sdk_v3::transaction::Transaction;

    let owner = keypair.pubkey();
    let wsol_mint = Pubkey::from_str(WSOL_MINT)?;
    let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)?;

    let accounts = rpc
        .get_token_accounts_by_owner(&owner, TokenAccountsFilter::Mint(wsol_mint))
        .await
        .map_err(|e| anyhow!("list wSOL token accounts: {}", e))?;

    let instructions: Vec<Instruction> = accounts
        .iter()
        .filter_map(|acc| Pubkey::from_str(&acc.pubkey).ok())
        .map(|account| close_wsol_account_ix(token_program, account, owner))
        .collect();

    if instructions.is_empty() {
        return Ok(None);
    }

    let blockhash = rpc
        .get_latest_blockhash()
        .await
        .map_err(|e| anyhow!("fetch blockhash for wSOL unwrap: {}", e))?;
    let tx = Transaction::new_signed_with_payer(&instructions, Some(&owner), &[keypair], blockhash);
    let signature = rpc
        .send_and_confirm_transaction(&tx)
        .await
        .map_err(|e| anyhow!("send wSOL unwrap tx: {}", e))?;

    Ok(Some(signature.to_string()))
}

pub async fn close_position(
    position_address: &str,
    config: &Config,
    rent_receiver: Option<&str>,
) -> Result<NativeCloseResult> {
    let wallet_secret = wallet_secret_from_env()?;
    let keypair = keypair_from_secret(&wallet_secret)?;
    let position = parse_pubkey("DLMM position address", position_address)?;
    let rent_receiver = rent_receiver
        .map(|value| parse_pubkey("rent receiver", value))
        .transpose()?;
    let rpc_url = resolve_rpc_url(config);
    let rpc_client = RpcClient::new(rpc_url);
    let rpc_ctx = RpcContext::confirmed(Arc::new(rpc_client));
    let params = ClosePositionParams {
        position_address: position,
        authority: keypair.pubkey(),
        rent_receiver,
    };
    let plan_config = WorkspacePlanConfig::default();
    let result = close_position_one_shot(&rpc_ctx, params, &plan_config, &keypair)
        .await
        .map_err(|e| anyhow!("native Meteora close_position_one_shot failed: {}", e))?;

    // Removing single-side SOL liquidity returns the principal as wrapped SOL
    // (wSOL) in a token account; the close itself does not unwrap it. Sweep any
    // wSOL accounts back to native SOL so the freed capital is spendable again.
    // Non-fatal: the position is already closed, so log and continue on failure.
    let unwrap_signature = match unwrap_wsol(&rpc_ctx.client, &keypair).await {
        Ok(Some(sig)) => {
            tracing::info!(unwrap_signature = %sig, "unwrapped wSOL to native SOL after close");
            Some(sig)
        }
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "failed to auto-unwrap wSOL after close (funds safe as wSOL)");
            None
        }
    };

    Ok(NativeCloseResult {
        signature: result.signature.to_string(),
        unwrap_signature,
        remove_liquidity_amount_x: result.quote.remove_liquidity_amount_x,
        remove_liquidity_amount_y: result.quote.remove_liquidity_amount_y,
        claimable_fee_x: result.quote.claimable_fee_x,
        claimable_fee_y: result.quote.claimable_fee_y,
        claimable_rewards: result.quote.claimable_rewards,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::{Keypair, Signer};

    #[test]
    fn keypair_pubkey_from_secret_matches_existing_wallet_secret() {
        let keypair = Keypair::new();
        let pubkey = keypair_pubkey_from_secret(&keypair.to_base58_string())
            .expect("v2 wallet secret should parse into native SDK keypair");

        assert_eq!(pubkey, keypair.pubkey().to_string());
    }

    #[test]
    fn close_wsol_account_ix_has_correct_shape() {
        let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID).unwrap();
        let account = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let ix = close_wsol_account_ix(token_program, account, owner);

        assert_eq!(ix.program_id, token_program);
        // CloseAccount discriminator, no extra payload.
        assert_eq!(ix.data, vec![SPL_TOKEN_CLOSE_ACCOUNT_IX]);
        assert_eq!(ix.accounts.len(), 3);
        // [0] account being closed — writable, not signer.
        assert_eq!(ix.accounts[0].pubkey, account);
        assert!(ix.accounts[0].is_writable);
        assert!(!ix.accounts[0].is_signer);
        // [1] lamports destination (owner) — writable, not signer.
        assert_eq!(ix.accounts[1].pubkey, owner);
        assert!(ix.accounts[1].is_writable);
        assert!(!ix.accounts[1].is_signer);
        // [2] owner authority — signer, read-only.
        assert_eq!(ix.accounts[2].pubkey, owner);
        assert!(ix.accounts[2].is_signer);
        assert!(!ix.accounts[2].is_writable);
    }

    #[test]
    fn build_claim_request_uses_position_and_wallet_authority() {
        let keypair = Keypair::new();
        let position = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let config = crate::config::Config {
            api: crate::config::types::ApiConfig {
                helius_rpc_url: Some("https://rpc.example.test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let request = build_claim_request(&position, &config, &keypair.to_base58_string())
            .expect("claim request should be built from string inputs");

        assert_eq!(request.position_address, position);
        assert_eq!(request.authority, keypair.pubkey().to_string());
        assert_eq!(request.rpc_url, "https://rpc.example.test");
    }

    #[test]
    fn resolve_rpc_url_prefers_config_then_env_then_default() {
        let config = crate::config::Config {
            api: crate::config::types::ApiConfig {
                helius_rpc_url: Some("https://configured-rpc.example.test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            resolve_rpc_url(&config),
            "https://configured-rpc.example.test"
        );
    }

    #[test]
    fn build_close_request_defaults_rent_receiver_to_authority() {
        let keypair = Keypair::new();
        let position = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let config = crate::config::Config {
            api: crate::config::types::ApiConfig {
                helius_rpc_url: Some("https://rpc.example.test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let request = build_close_request(&position, &config, &keypair.to_base58_string(), None)
            .expect("close request should be built from string inputs");
        let authority = keypair.pubkey().to_string();

        assert_eq!(request.position_address, position);
        assert_eq!(request.authority, authority);
        assert_eq!(request.rent_receiver.as_deref(), Some(authority.as_str()));
        assert_eq!(request.rpc_url, "https://rpc.example.test");
    }

    #[test]
    fn build_deploy_request_maps_sol_amount_bins_and_strategy() {
        let keypair = Keypair::new();
        let pool = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let config = crate::config::Config {
            api: crate::config::types::ApiConfig {
                helius_rpc_url: Some("https://rpc.example.test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let request = build_deploy_request(
            NativeDeployBuildInput {
                pool_address: &pool,
                amount_sol: 0.25,
                active_id: 100,
                bins_below: 35,
                bins_above: 0,
                strategy: "bid_ask",
            },
            &config,
            &keypair.to_base58_string(),
        )
        .expect("deploy request should be built from string inputs");

        assert_eq!(request.pool_address, pool);
        assert_eq!(request.authority, keypair.pubkey().to_string());
        assert_eq!(request.amount_x, 0);
        assert_eq!(request.amount_y, 250_000_000);
        assert_eq!(request.active_id, 100);
        assert_eq!(request.min_bin_id, 65);
        assert_eq!(request.max_bin_id, 100);
        assert_eq!(request.width, 36);
        assert_eq!(request.strategy, "bid_ask_imbalanced");
        assert_eq!(request.rpc_url, "https://rpc.example.test");
    }
}
