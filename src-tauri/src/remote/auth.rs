//! Native authentication boundary, independent of the eventual LAN listener.
use super::{
    event_hub::{EventHub, Subscription},
    protocol::*,
    store::{CredentialHash, RemoteStore},
};
use rusqlite::Connection;
use std::sync::Arc;

/// The hub-owned synchronous gate makes admission and
/// revocation indivisible; callers must never hold it across network waits.
pub struct AuthManager {
    identity: Id,
    hub: Arc<EventHub>,
}
impl AuthManager {
    pub fn new(identity: &Id, hub: Arc<EventHub>) -> Result<Self, RemoteError> {
        hub.bind_identity(identity)?;
        Ok(Self {
            identity: identity.clone(),
            hub,
        })
    }
    pub fn authenticate_bearer(
        &self,
        db: &mut Connection,
        bearer: &BearerCredential,
    ) -> Result<PairedDevice, RemoteError> {
        let _guard = self.hub.admission.lock().unwrap();
        authenticate_bearer(&RemoteStore::new(db, &self.identity), bearer)
    }
    pub fn authenticate_subscription(
        &self,
        db: &mut Connection,
        bearer: &BearerCredential,
    ) -> Result<(PairedDevice, Subscription), RemoteError> {
        let _guard = self.hub.admission.lock().unwrap();
        let device = authenticate_bearer(&RemoteStore::new(db, &self.identity), bearer)?;
        let subscription = self.hub.subscribe_owner(Some(device.device_id.clone()));
        Ok((device, subscription))
    }
    /// Called only after native confirmation validation; supports self-revocation.
    pub fn revoke_device(
        &self,
        db: &mut Connection,
        device: &Id,
        now_ms: i64,
    ) -> Result<bool, RemoteError> {
        let _guard = self.hub.admission.lock().unwrap();
        let revoked = RemoteStore::new(db, &self.identity)
            .revoke_device(device, now_ms)
            .map_err(|_| RemoteError::new(ErrorCode::Internal, "Could not revoke device."))?;
        self.hub.close_device(device);
        Ok(revoked)
    }
}
pub fn authenticate_bearer(
    store: &RemoteStore<'_>,
    bearer: &BearerCredential,
) -> Result<PairedDevice, RemoteError> {
    store
        .lookup_credential(&CredentialHash::from_credential(bearer))
        .map_err(|_| RemoteError::new(ErrorCode::Internal, "Could not authenticate device."))?
        .ok_or_else(|| RemoteError::new(ErrorCode::Unauthorized, "Pair this device again."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::store::*;
    use rusqlite::Connection;
    use serde_json::json;
    #[tokio::test]
    async fn revoking_a_device_closes_its_existing_subscription() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
            .unwrap();
        let server: Id = serde_json::from_value(json!("server")).unwrap();
        let device: PairedDevice = serde_json::from_value(json!({"deviceId":"phone","label":"Phone","scopes":["browse"],"createdAtMs":1,"revoked":false})).unwrap();
        let bearer: BearerCredential = serde_json::from_value(json!("a".repeat(43))).unwrap();
        let hub = Arc::new(EventHub::default());
        let auth = AuthManager::new(&server, hub).unwrap();
        let mut store = RemoteStore::new(&mut db, &server);
        store
            .add_device(&device, &CredentialHash::from_credential(&bearer))
            .unwrap();
        let (_, mut subscription) = auth.authenticate_subscription(&mut db, &bearer).unwrap();
        let waiting = tokio::spawn(async move { subscription.recv().await });
        tokio::task::yield_now().await;
        auth.revoke_device(&mut db, &device.device_id, 2).unwrap();
        assert!(auth.authenticate_bearer(&mut db, &bearer).is_err());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), waiting)
                .await
                .unwrap()
                .unwrap()
                .is_err(),
            "revoked subscription must immediately stop delivering events"
        );
    }
}
