use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub pool_address: String,
    pub base_mint: String,
    pub symbol: Option<String>,
    pub notes: Vec<PoolNote>,
    pub last_yield: Option<String>,
    pub last_close_reason: Option<String>,
    pub last_close_pnl: Option<f64>,
    pub last_close_at: Option<String>,
    pub total_positions: u32,
    pub total_fees_earned: f64,
    pub on_cooldown: bool,
    pub cooldown_until: Option<String>,
    pub cooldown_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolNote {
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PoolMemoryStore {
    pub pools: HashMap<String, PoolEntry>,
}

impl PoolMemoryStore {
    pub fn load(path: &str) -> Result<Self> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content).unwrap_or_default())
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &str) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, pool_address: &str) -> Option<&PoolEntry> {
        self.pools.get(pool_address)
    }

    pub fn add_note(
        &mut self,
        pool_address: &str,
        base_mint: &str,
        symbol: Option<&str>,
        content: &str,
    ) {
        let entry = self
            .pools
            .entry(pool_address.to_string())
            .or_insert_with(|| PoolEntry {
                pool_address: pool_address.to_string(),
                base_mint: base_mint.to_string(),
                symbol: symbol.map(String::from),
                notes: vec![],
                last_yield: None,
                last_close_reason: None,
                last_close_pnl: None,
                last_close_at: None,
                total_positions: 0,
                total_fees_earned: 0.0,
                on_cooldown: false,
                cooldown_until: None,
                cooldown_reason: None,
            });
        entry.notes.push(PoolNote {
            content: content.to_string(),
            created_at: Utc::now().to_rfc3339(),
        });
    }

    pub fn record_close(
        &mut self,
        pool_address: &str,
        base_mint: &str,
        symbol: Option<&str>,
        reason: &str,
        pnl: f64,
    ) {
        let entry = self
            .pools
            .entry(pool_address.to_string())
            .or_insert_with(|| PoolEntry {
                pool_address: pool_address.to_string(),
                base_mint: base_mint.to_string(),
                symbol: symbol.map(String::from),
                notes: vec![],
                last_yield: None,
                last_close_reason: None,
                last_close_pnl: None,
                last_close_at: None,
                total_positions: 0,
                total_fees_earned: 0.0,
                on_cooldown: false,
                cooldown_until: None,
                cooldown_reason: None,
            });
        entry.last_close_reason = Some(reason.to_string());
        entry.last_close_pnl = Some(pnl);
        entry.last_close_at = Some(Utc::now().to_rfc3339());
        entry.total_positions += 1;
    }

    pub fn is_on_cooldown(&self, base_mint: &str) -> bool {
        self.pools
            .values()
            .filter(|p| p.base_mint == base_mint && p.on_cooldown)
            .any(|p| {
                if let Some(ref until) = p.cooldown_until {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(until) {
                        return dt > Utc::now();
                    }
                }
                false
            })
    }

    pub fn set_cooldown(&mut self, base_mint: &str, reason: &str, minutes: u32) {
        let until = (Utc::now() + chrono::Duration::minutes(minutes as i64)).to_rfc3339();
        for (_, entry) in self.pools.iter_mut() {
            if entry.base_mint == base_mint {
                entry.on_cooldown = true;
                entry.cooldown_until = Some(until.clone());
                entry.cooldown_reason = Some(reason.to_string());
            }
        }
    }

    pub fn clear_cooldown(&mut self, base_mint: &str) {
        for (_, entry) in self.pools.iter_mut() {
            if entry.base_mint == base_mint {
                entry.on_cooldown = false;
                entry.cooldown_until = None;
                entry.cooldown_reason = None;
            }
        }
    }

    pub fn get_summary_for_prompt(&self) -> String {
        if self.pools.is_empty() {
            return "No pool history".to_string();
        }
        let mut lines = Vec::new();
        for entry in self.pools.values() {
            if entry.total_positions > 0 || !entry.notes.is_empty() {
                let mut line = format!(
                    "{}: {} positions, {:.4} SOL fees",
                    entry.symbol.as_deref().unwrap_or(&entry.base_mint[..8]),
                    entry.total_positions,
                    entry.total_fees_earned
                );
                if let Some(ref reason) = entry.last_close_reason {
                    line += &format!(", last close: {}", reason);
                }
                if entry.on_cooldown {
                    line += " [COOLDOWN]";
                }
                lines.push(line);
            }
        }
        lines.join("\n")
    }
}
