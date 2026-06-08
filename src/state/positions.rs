use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::Utc;

// Re-export the position types
pub use crate::models::position::{TrackedPosition, PositionStatus};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PositionState {
    pub positions: HashMap<String, TrackedPosition>,
}

impl PositionState {
    pub fn load(path: &str) -> Result<Self> {
        let p = Path::new(path);
        if p.exists() {
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

    pub fn add(&mut self, pos: TrackedPosition) {
        self.positions.insert(pos.id.clone(), pos);
    }

    pub fn remove(&mut self, id: &str) {
        self.positions.remove(id);
    }

    pub fn get_active(&self) -> Vec<&TrackedPosition> {
        self.positions.values()
            .filter(|p| p.status == PositionStatus::Active || p.status == PositionStatus::OutOfRange)
            .collect()
    }

    pub fn count_active(&self) -> usize {
        self.positions.values()
            .filter(|p| p.status == PositionStatus::Active || p.status == PositionStatus::OutOfRange)
            .count()
    }

    pub fn mark_oor(&mut self, id: &str) {
        if let Some(p) = self.positions.get_mut(id) {
            if p.status == PositionStatus::Active {
                p.status = PositionStatus::OutOfRange;
                p.out_of_range_since = Some(Utc::now().to_rfc3339());
            }
        }
    }

    pub fn mark_in_range(&mut self, id: &str) {
        if let Some(p) = self.positions.get_mut(id) {
            if p.status == PositionStatus::OutOfRange {
                p.status = PositionStatus::Active;
                p.out_of_range_since = None;
            }
        }
    }

    pub fn record_claim(&mut self, id: &str, fees: f64) {
        if let Some(p) = self.positions.get_mut(id) {
            p.total_fees_claimed += fees;
        }
    }

    pub fn record_close(&mut self, id: &str, pnl: f64) {
        if let Some(p) = self.positions.get_mut(id) {
            p.status = PositionStatus::Closed;
            p.pnl_sol = Some(pnl);
        }
    }
}
