//! One active native setup window. Secret material never enters diagnostics or SQL.
use super::{
    adapters::checked,
    protocol::*,
    store::{CredentialHash, RemoteStore},
    tls::ServerIdentity,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    net::IpAddr,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
type Result<T> = std::result::Result<T, RemoteError>;
enum Submission<'a> {
    Qr(&'a PairRequest),
    Manual(&'a ManualPairRequest),
}
struct Attempt {
    id: Id,
    expires: Instant,
    qr_hash: [u8; 32],
    manual_hash: [u8; 32],
    failures: u8,
}
#[derive(Debug)]
pub struct PairingReady {
    pub candidate: PairingCandidate,
    pub manual_code: ManualCode,
    pub verification_phrase: VerificationPhrase,
}
pub struct PairingManager {
    identity: Id,
    fingerprint: Fingerprint,
    phrase: VerificationPhrase,
    attempt: Option<Attempt>,
    failures: VecDeque<(Instant, IpAddr)>,
    #[cfg(test)]
    offset: Duration,
    #[cfg(test)]
    test_start: Instant,
}
impl PairingManager {
    pub fn new(identity: &ServerIdentity) -> Self {
        Self {
            identity: identity.id().clone(),
            fingerprint: identity.fingerprint(),
            phrase: super::tls::verification_phrase(
                &Sha256::digest(identity.certificate().as_ref()).into(),
            ),
            attempt: None,
            failures: VecDeque::new(),
            #[cfg(test)]
            offset: Duration::ZERO,
            #[cfg(test)]
            test_start: Instant::now(),
        }
    }
    fn now(&self) -> Instant {
        #[cfg(test)]
        {
            self.test_start + self.offset
        }
        #[cfg(not(test))]
        {
            Instant::now()
        }
    }
    #[cfg(test)]
    fn advance_for_test(&mut self, delta: Duration) {
        self.offset += delta;
    }
    /// Native setup action only. A paired phone must complete its confirmation
    /// before the command handler calls this method and records actual issuance.
    pub fn begin(&mut self, addresses: LocalAddresses, port: Port) -> Result<PairingReady> {
        let secret: PairingSecret = checked(random_token()?)?;
        let mut entropy = [0u8; 12];
        getrandom::fill(&mut entropy).map_err(|_| internal())?;
        const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let code: ManualCode = checked(
            entropy
                .iter()
                .map(|b| BASE32[(b & 31) as usize] as char)
                .collect::<String>(),
        )?;
        let attempt_id: Id = checked(uuid::Uuid::new_v4().to_string())?;
        self.attempt = Some(Attempt {
            id: attempt_id.clone(),
            expires: self.now() + Duration::from_secs(120),
            qr_hash: hash(secret.as_str()),
            manual_hash: hash(code.as_str()),
            failures: 0,
        });
        Ok(PairingReady {
            candidate: PairingCandidate {
                v: checked(1)?,
                device_id: self.identity.clone(),
                addresses,
                port,
                certificate_fingerprint: self.fingerprint.clone(),
                attempt_id,
                secret,
            },
            manual_code: code,
            verification_phrase: self.phrase.clone(),
        })
    }
    /// Read-only native setup projection; validates the monotonic deadline.
    pub fn active_attempt(&mut self) -> Option<(Id, Duration)> {
        let now = self.now();
        self.active().ok().map(|attempt| {
            (
                attempt.id.clone(),
                attempt.expires.saturating_duration_since(now),
            )
        })
    }
    pub fn close_attempt(&mut self, id: &Id) {
        if self
            .attempt
            .as_ref()
            .is_some_and(|attempt| &attempt.id == id)
        {
            self.close();
        }
    }
    pub fn close(&mut self) {
        self.attempt = None;
    }
    fn active(&mut self) -> Result<&mut Attempt> {
        let now = self.now();
        if self.attempt.as_ref().is_some_and(|a| now >= a.expires) {
            self.close();
        }
        self.attempt.as_mut().ok_or_else(denied)
    }
    pub fn manual_challenge(&mut self) -> Result<ManualChallengeResponse> {
        let attempt_id = self.active()?.id.clone();
        Ok(ManualChallengeResponse {
            v: checked(1)?,
            server_device_id: self.identity.clone(),
            attempt_id,
        })
    }
    pub fn pair_qr(
        &mut self,
        peer: IpAddr,
        request: &PairRequest,
        db: &mut Connection,
        now_ms: i64,
    ) -> Result<PairedMediacenter> {
        self.pair(peer, Submission::Qr(request), db, now_ms)
    }
    pub fn pair_manual(
        &mut self,
        peer: IpAddr,
        request: &ManualPairRequest,
        db: &mut Connection,
        now_ms: i64,
    ) -> Result<PairedMediacenter> {
        self.pair(peer, Submission::Manual(request), db, now_ms)
    }
    fn pair(
        &mut self,
        peer: IpAddr,
        submission: Submission<'_>,
        db: &mut Connection,
        now_ms: i64,
    ) -> Result<PairedMediacenter> {
        let (id, secret, manual, label) = match submission {
            Submission::Qr(r) => (&r.attempt_id, r.secret.as_str(), false, &r.device_label),
            Submission::Manual(r) => (&r.attempt_id, r.code.as_str(), true, &r.device_label),
        };
        let now = self.now();
        while self
            .failures
            .front()
            .is_some_and(|(t, _)| now.duration_since(*t) >= Duration::from_secs(60))
        {
            self.failures.pop_front();
        }
        if self.failures.len() >= 20
            || self.failures.iter().filter(|(_, p)| *p == peer).count() >= 5
        {
            return Err(denied());
        }
        let accepted = self.active().is_ok_and(|attempt| {
            let expected = if manual {
                &attempt.manual_hash
            } else {
                &attempt.qr_hash
            };
            let secret_matches = bool::from(expected.ct_eq(&hash(secret)));
            if attempt.id != *id || !secret_matches {
                if attempt.id == *id {
                    attempt.failures += 1;
                }
                false
            } else {
                true
            }
        });
        if !accepted {
            self.failures.push_back((now, peer));
            if self.attempt.as_ref().is_some_and(|a| a.failures >= 5) {
                self.close();
            }
            return Err(denied());
        }
        let credential: BearerCredential = checked(random_token()?)?;
        let scopes: ScopeSet = checked(serde_json::json!([
            "browse",
            "playback",
            "navigation",
            "outputs",
            "library_manage",
            "settings",
            "browser_control"
        ]))?;
        let device = PairedDevice {
            device_id: checked(uuid::Uuid::new_v4().to_string())?,
            label: label.clone(),
            scopes: scopes.clone(),
            created_at_ms: checked(now_ms)?,
            last_seen_at_ms: None,
            revoked: false,
        };
        RemoteStore::new(db, &self.identity)
            .add_device(&device, &CredentialHash::from_credential(&credential))
            .map_err(|e| {
                if e.code == "busy" {
                    RemoteError::new(
                        ErrorCode::Busy,
                        "Remove a paired phone before adding another.",
                    )
                } else {
                    internal()
                }
            })?;
        self.close();
        Ok(PairedMediacenter {
            server_device_id: self.identity.clone(),
            device_id: device.device_id,
            credential,
            certificate_fingerprint: self.fingerprint.clone(),
            scopes,
        })
    }
}
fn hash(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}
fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| internal())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn denied() -> RemoteError {
    RemoteError::new(
        ErrorCode::Unauthorized,
        "Pairing is unavailable or the attempt is invalid.",
    )
}
fn internal() -> RemoteError {
    RemoteError::new(ErrorCode::Internal, "Pairing could not be completed.")
}
#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;

#[derive(Debug)]
pub enum PairEndpointRequest {
    Qr(PairRequest),
    Manual(ManualPairRequest),
    ManualChallenge(ManualChallengeRequest),
}
/// The only unauthenticated request boundary, bounded before JSON allocation.
pub fn parse_pair_request(input: &str) -> Result<PairEndpointRequest> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(RemoteError::invalid());
    }
    let strict: super::protocol_json::StrictValue =
        serde_json::from_str(input).map_err(|_| RemoteError::invalid())?;
    let object = strict.0.as_object().ok_or_else(RemoteError::invalid)?;
    let fields: &[&str] = if object.contains_key("mode") {
        &["v", "mode"]
    } else if object.contains_key("code") {
        &["attemptId", "code", "deviceLabel"]
    } else {
        &["attemptId", "secret", "deviceLabel"]
    };
    if object.len() != fields.len() || object.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err(RemoteError::invalid());
    }
    if object.contains_key("mode") {
        Ok(PairEndpointRequest::ManualChallenge(checked(strict.0)?))
    } else if object.contains_key("code") {
        Ok(PairEndpointRequest::Manual(checked(strict.0)?))
    } else {
        Ok(PairEndpointRequest::Qr(checked(strict.0)?))
    }
}

/// Encode only the newly issued candidate; QR material has the same secret lifetime.
pub(crate) fn qr_rows(candidate: &PairingCandidate) -> Result<QrRows> {
    let json = serde_json::to_vec(candidate).map_err(|_| internal())?;
    let qr = qrcode::QrCode::new(&json).map_err(|_| internal())?;
    let width = qr.width();
    checked(
        qr.into_colors()
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
                    .collect::<String>()
            })
            .collect::<Vec<_>>(),
    )
}
