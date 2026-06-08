use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub content: String,
    pub pinned: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LessonStore {
    pub lessons: Vec<Lesson>,
}

impl LessonStore {
    pub fn load(path: &str) -> Result<Self> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &str) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add(&mut self, content: &str) {
        self.lessons.push(Lesson {
            id: format!("lesson_{}", chrono::Utc::now().timestamp_millis()),
            content: content.to_string(),
            pinned: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    pub fn pin(&mut self, id: &str) -> bool {
        if let Some(l) = self.lessons.iter_mut().find(|l| l.id == id) {
            l.pinned = true;
            true
        } else { false }
    }

    pub fn unpin(&mut self, id: &str) -> bool {
        if let Some(l) = self.lessons.iter_mut().find(|l| l.id == id) {
            l.pinned = false;
            true
        } else { false }
    }

    pub fn clear(&mut self) {
        self.lessons.retain(|l| l.pinned);
    }

    pub fn get_for_prompt(&self) -> String {
        if self.lessons.is_empty() { return String::new(); }
        self.lessons.iter()
            .map(|l| format!("- {}{}", if l.pinned { "[PINNED] " } else { "" }, l.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
