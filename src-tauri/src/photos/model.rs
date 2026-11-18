use crate::contracts::AppError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SlideshowRequest {
    pub media_ids: Vec<String>,
    pub monitor_id: Option<String>,
    #[serde(default = "default_interval")]
    pub interval_seconds: u32,
    #[serde(default)]
    pub shuffle: bool,
}
fn default_interval() -> u32 {
    5
}
pub struct Queue {
    pub ids: Vec<String>,
    original: Vec<String>,
    pub index: usize,
    pub paused: bool,
    pub interval: u32,
    pub shuffled: bool,
}
impl Queue {
    pub fn new(request: SlideshowRequest) -> Result<Self, AppError> {
        if request.media_ids.is_empty()
            || request.media_ids.len() > 10_000
            || request
                .media_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 200)
        {
            return Err(AppError::new(
                "invalid_slideshow",
                "Choose between 1 and 10,000 photos.",
            ));
        }
        validate_interval(request.interval_seconds)?;
        let mut result = Self {
            original: request.media_ids.clone(),
            ids: request.media_ids,
            index: 0,
            paused: false,
            interval: request.interval_seconds,
            shuffled: false,
        };
        result.shuffle(request.shuffle);
        Ok(result)
    }
    pub fn current(&self) -> &str {
        &self.ids[self.index]
    }
    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.ids.len();
    }
    pub fn previous(&mut self) {
        self.index = (self.index + self.ids.len() - 1) % self.ids.len();
    }
    pub fn preloads(&self) -> Vec<&str> {
        (1..self.ids.len().min(3))
            .map(|offset| self.ids[(self.index + offset) % self.ids.len()].as_str())
            .collect()
    }
    pub fn pause(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn shuffle(&mut self, enabled: bool) {
        if self.shuffled == enabled {
            return;
        }
        let current = self.current().to_string();
        self.ids = self.original.clone();
        if enabled {
            // OS-random UUID bytes avoid a new RNG dependency; Fisher-Yates retains all entries.
            for i in (1..self.ids.len()).rev() {
                let random = uuid::Uuid::new_v4().as_u128();
                self.ids.swap(i, (random % (i as u128 + 1)) as usize);
            }
        }
        self.index = self.ids.iter().position(|id| id == &current).unwrap_or(0);
        self.shuffled = enabled;
    }
}
pub fn validate_interval(seconds: u32) -> Result<(), AppError> {
    if !(1..=60).contains(&seconds) {
        Err(AppError::new(
            "invalid_interval",
            "Choose an interval from 1 to 60 seconds.",
        ))
    } else {
        Ok(())
    }
}
