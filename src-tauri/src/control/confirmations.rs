//! Owner-bound, expiring native confirmations. Actions stay on the server.
use super::activity::Identity;
use crate::remote::protocol::*;
#[derive(Clone)]
pub enum Action {
    Activity {
        target: ActivityPrepareParams,
        media: super::activity::MediaIdentity,
    },
    Audio {
        output: crate::contracts::AudioSinkDescriptor,
        sessions: super::audio::Sessions,
    },
}
#[derive(Clone)]
pub struct Pending {
    pub preview: ConfirmationPreview,
    pub action: Action,
    pub observed: Identity,
    pub request: Id,
    pub expires_at: u64,
    pub operation: Option<Id>,
}
#[derive(Default)]
pub struct Confirmations {
    pub(crate) pending: std::collections::HashMap<String, Pending>,
    pub(crate) automatic: std::collections::HashMap<(String, String), u64>,
}
impl Confirmations {
    pub fn get(&self, owner: &Id, id: &Id, now: u64) -> Result<Pending, RemoteError> {
        let pending = self
            .pending
            .get(id.as_str())
            .filter(|p| &p.preview.owner_device_id == owner)
            .ok_or_else(|| {
                RemoteError::new(ErrorCode::NotFound, "The confirmation was not found.")
            })?;
        if now >= pending.expires_at {
            return Err(RemoteError::new(
                ErrorCode::ConfirmationExpired,
                "Review the activity again.",
            ));
        }
        let mut value = pending.clone();
        value.preview.expires_in_ms =
            crate::remote::adapters::checked(pending.expires_at.saturating_sub(now).min(60000))
                .expect("bounded TTL");
        Ok(value)
    }
    pub fn insert(&mut self, pending: Pending, now: u64) -> Result<(), RemoteError> {
        self.pending.retain(|_, p| p.expires_at > now);
        // Exactly one active sheet per owner; retain consumed IDs briefly for double-clicks.
        self.pending.retain(|_, p| {
            p.preview.owner_device_id != pending.preview.owner_device_id || p.operation.is_some()
        });
        if self.pending.len() >= 256 {
            return Err(RemoteError::new(ErrorCode::Busy, "Too many confirmations."));
        }
        self.pending
            .insert(pending.preview.confirmation_id.as_str().into(), pending);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::adapters::checked;
    #[test]
    fn task9_confirmation_ownership_expiry_and_consumption_are_server_bound() {
        let mut registry = Confirmations::default();
        let owner: Id = checked("owner").unwrap();
        let id: Id = checked("confirmation").unwrap();
        let preview=checked(serde_json::json!({"confirmationId":id,"ownerDeviceId":owner,"action":"replace_activity","affectedLabels":["Photo"],"expected":{"serverEpoch":"epoch"},"expiresInMs":60000,"confirmMethod":"activity.confirm"})).unwrap();
        registry.pending.insert(
            id.as_str().into(),
            Pending {
                preview,
                action: Action::Activity {
                    target: checked(serde_json::json!({"kind":"photo","itemId":"photo"})).unwrap(),
                    media: Default::default(),
                },
                observed: Identity::default(),
                request: checked("request").unwrap(),
                expires_at: 60001,
                operation: None,
            },
        );
        assert_eq!(
            registry
                .get(&checked("other").unwrap(), &id, 1)
                .err()
                .unwrap()
                .code,
            ErrorCode::NotFound
        );
        assert!(registry.get(&owner, &id, 60000).is_ok());
        assert_eq!(
            registry.get(&owner, &id, 60001).err().unwrap().code,
            ErrorCode::ConfirmationExpired
        );
    }
}
