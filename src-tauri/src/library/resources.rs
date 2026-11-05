use crate::contracts::AppError;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[derive(Default)]
struct Registry {
    blocked: HashMap<String, usize>,
    active: HashMap<String, (String, CancellationToken)>,
}
#[derive(Clone, Default)]
pub struct Resources(Arc<Mutex<Registry>>);
pub struct ResourceLease {
    resources: Resources,
    id: String,
    cancel: CancellationToken,
}
pub struct ResourceBlock {
    resources: Resources,
    sources: Vec<String>,
}
impl ResourceLease {
    pub fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
    pub fn token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}
impl Drop for ResourceLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.resources.0.lock() {
            state.active.remove(&self.id);
        }
    }
}
impl Drop for ResourceBlock {
    fn drop(&mut self) {
        if let Ok(mut state) = self.resources.0.lock() {
            for source in &self.sources {
                if let Some(count) = state.blocked.get_mut(source) {
                    *count -= 1;
                    if *count == 0 {
                        state.blocked.remove(source);
                    }
                }
            }
        }
    }
}
impl Resources {
    pub fn active_count(&self, sources: &[String]) -> Result<usize, AppError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| AppError::new("resource_state_error", "Media resources are unavailable."))?
            .active
            .values()
            .filter(|(source, _)| sources.contains(source))
            .count())
    }

    pub fn acquire(
        &self,
        source: &str,
        cancel: CancellationToken,
    ) -> Result<ResourceLease, AppError> {
        let mut state = self.0.lock().map_err(|_| {
            AppError::new("resource_state_error", "Media resources are unavailable.")
        })?;
        if state.blocked.contains_key(source) || cancel.is_cancelled() {
            return Err(AppError::new(
                "source_busy",
                "This source is being stopped or safely removed.",
            ));
        }
        if state.active.len() >= 128 {
            return Err(AppError::new(
                "resource_limit",
                "Too many media reads are active.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        state
            .active
            .insert(id.clone(), (source.into(), cancel.clone()));
        Ok(ResourceLease {
            resources: self.clone(),
            id,
            cancel,
        })
    }
    pub fn block(&self, sources: &[String]) -> Result<ResourceBlock, AppError> {
        let mut state = self.0.lock().map_err(|_| {
            AppError::new("resource_state_error", "Media resources are unavailable.")
        })?;
        for source in sources {
            *state.blocked.entry(source.clone()).or_default() += 1;
        }
        for (source, cancel) in state.active.values() {
            if sources.contains(source) {
                cancel.cancel();
            }
        }
        Ok(ResourceBlock {
            resources: self.clone(),
            sources: sources.to_vec(),
        })
    }
    pub async fn wait_idle(&self, sources: &[String], timeout: Duration) -> Result<(), AppError> {
        tokio::time::timeout(timeout, async {
            loop {
                let active = self
                    .0
                    .lock()
                    .map_err(|_| {
                        AppError::new("resource_state_error", "Media resources are unavailable.")
                    })?
                    .active
                    .values()
                    .any(|(source, _)| sources.contains(source));
                if !active {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| {
            AppError::new(
                "source_busy",
                "Media work has not finished closing. The drive was not unmounted.",
            )
        })?
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn removal_cancels_existing_and_blocks_new_readers() {
        let resources = Resources::default();
        let token = CancellationToken::new();
        let lease = resources.acquire("source", token.clone()).unwrap();
        let sources = vec!["source".into()];
        let block = resources.block(&sources).unwrap();
        assert!(token.is_cancelled());
        assert!(resources
            .acquire("source", CancellationToken::new())
            .is_err());
        assert!(resources
            .wait_idle(&sources, Duration::from_millis(30))
            .await
            .is_err());
        drop(lease);
        resources
            .wait_idle(&sources, Duration::from_millis(30))
            .await
            .unwrap();
        drop(block);
        assert!(resources
            .acquire("source", CancellationToken::new())
            .is_ok());
    }
}
