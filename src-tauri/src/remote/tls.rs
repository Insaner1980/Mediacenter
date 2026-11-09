//! Installation identity and strict certificate pinning. No listener is started here.
use super::{adapters::checked, protocol::*};
use rusqlite::{params, Connection};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
};
use subtle::ConstantTimeEq;
type Result<T> = std::result::Result<T, RemoteError>;
fn failure() -> RemoteError {
    RemoteError::new(
        ErrorCode::Internal,
        "Remote identity could not be loaded securely.",
    )
}
#[derive(Clone, Copy)]
pub enum IdentityNamespace {
    Production,
    Development,
}
impl IdentityNamespace {
    fn directory(self) -> &'static str {
        match self {
            Self::Production => "remote-production",
            Self::Development => "remote-development",
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityFile {
    certificate: Vec<u8>,
    private_key: Vec<u8>,
}
/// Private key material is deliberately neither Serialize nor Debug.
pub struct ServerIdentity {
    id: Id,
    certificate: CertificateDer<'static>,
    config: Arc<ServerConfig>,
    repair: bool,
}
impl ServerIdentity {
    /// Call once during native startup, before constructing the state/hub and
    /// before admitting clients. Missing key recovery preserves revoked records.
    pub fn load_or_create(
        root: &Path,
        namespace: IdentityNamespace,
        db: &mut Connection,
        now_ms: i64,
    ) -> Result<Self> {
        let directory = root.join(namespace.directory());
        match fs::symlink_metadata(&directory) {
            Ok(m) if !m.is_dir() || m.file_type().is_symlink() => return Err(failure()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .map_err(|_| failure())?,
            Err(_) => return Err(failure()),
        }
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| failure())?;
        let id_path = directory.join("server-id");
        let bundle_path = directory.join("identity.json");
        let (id, existed): (Id, bool) = match read_private(&id_path)? {
            Some(bytes) => (
                checked(String::from_utf8(bytes).map_err(|_| failure())?)?,
                true,
            ),
            None => {
                if fs::symlink_metadata(&bundle_path).is_ok() {
                    return Err(failure());
                }
                let value = uuid::Uuid::new_v4().to_string();
                write_private(&id_path, value.as_bytes())?;
                (checked(value)?, false)
            }
        };
        let (bundle, repair) = match read_private(&bundle_path)? {
            Some(bytes) => (
                serde_json::from_slice::<IdentityFile>(&bytes).map_err(|_| failure())?,
                false,
            ),
            None => {
                // Revoke BEFORE new key publication. A crash cannot leave the new
                // key able to accept credentials bound to a lost certificate.
                db.execute("UPDATE paired_devices SET revoked_at_ms=?1 WHERE server_identity=?2 AND revoked_at_ms IS NULL",params![now_ms,id.as_str()]).map_err(|_|failure())?;
                let key = rcgen::KeyPair::generate().map_err(|_| failure())?;
                let params = rcgen::CertificateParams::new(vec!["mediacenter.local".to_owned()])
                    .map_err(|_| failure())?;
                let cert = params.self_signed(&key).map_err(|_| failure())?;
                let bundle = IdentityFile {
                    certificate: cert.der().to_vec(),
                    private_key: key.serialize_der(),
                };
                write_private(
                    &bundle_path,
                    &serde_json::to_vec(&bundle).map_err(|_| failure())?,
                )?;
                (bundle, existed)
            }
        };
        let certificate = CertificateDer::from(bundle.certificate);
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| failure())?
                .with_no_client_auth()
                .with_single_cert(
                    vec![certificate.clone()],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bundle.private_key)),
                )
                .map_err(|_| failure())?;
        Ok(Self {
            id,
            certificate,
            config: Arc::new(config),
            repair,
        })
    }
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn certificate(&self) -> &CertificateDer<'static> {
        &self.certificate
    }
    pub fn fingerprint(&self) -> Fingerprint {
        fingerprint(self.certificate.as_ref())
    }
    pub fn server_config(&self) -> Arc<ServerConfig> {
        self.config.clone()
    }
    pub fn requires_repairing(&self) -> bool {
        self.repair
    }
}
fn read_private(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(failure()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 65536 {
        return Err(failure());
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| failure())?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| failure())?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| failure())?;
    if bytes.len() > 65536 {
        return Err(failure());
    }
    Ok(Some(bytes))
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| failure())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| failure())?;
    fs::rename(&temporary, path).map_err(|_| failure())?;
    fs::File::open(path.parent().ok_or_else(failure)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| failure())?;
    Ok(())
}
pub fn fingerprint(der: &[u8]) -> Fingerprint {
    checked(
        Sha256::digest(der)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
    .expect("SHA256 encoding")
}
pub fn verification_phrase(digest: &[u8; 32]) -> VerificationPhrase {
    let words: Vec<_> = include_str!("wordlist/bip39-en-v1.txt").lines().collect();
    let selected: Vec<_> = (0..6)
        .map(|word| {
            let index = (0..11).fold(0usize, |v, bit| {
                let position = word * 11 + bit;
                (v << 1) | ((digest[position / 8] >> (7 - position % 8)) & 1) as usize
            });
            words[index]
        })
        .collect();
    checked(serde_json::json!({"wordlistVersion":"bip39-en-v1","words":selected}))
        .expect("fixed wordlist")
}

/// Only this verifier authorizes application data. Signature verification is
/// still performed by rustls: knowing a certificate alone is insufficient.
#[derive(Debug)]
struct PinnedVerifier {
    pin: Fingerprint,
}
impl rustls::client::danger::ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        if bool::from(
            fingerprint(end.as_ref())
                .as_str()
                .as_bytes()
                .ct_eq(self.pin.as_str().as_bytes()),
        ) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("certificate pin mismatch".into()))
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
pub fn pinned_client_config(pin: Fingerprint) -> Arc<rustls::ClientConfig> {
    Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("supported TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier { pin }))
        .with_no_client_auth(),
    )
}
#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;

/// Manual bootstrap intentionally fails TLS before any application request.
/// The certificate is untrusted until the human confirms its locally derived phrase.
#[derive(Default)]
pub struct CertificateProbe {
    observed: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}
impl CertificateProbe {
    pub fn client_config(&self) -> Arc<rustls::ClientConfig> {
        Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("supported TLS versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(ProbeVerifier(self.observed.clone())))
            .with_no_client_auth(),
        )
    }
    /// Capture once for the confirmation UI. Later/retried probes may replace
    /// the latest observation, but cannot change this value's phrase or pin.
    pub fn observation(&self) -> Option<CertificateObservation> {
        self.observed
            .lock()
            .unwrap()
            .as_ref()
            .map(|der| CertificateObservation {
                phrase: verification_phrase(&Sha256::digest(der).into()),
                pin: fingerprint(der),
            })
    }
}
/// An immutable certificate observation: display this phrase and retain this
/// exact object until the user confirms or rejects it. It carries no shared
/// reference to the probe's mutable observation slot.
pub struct CertificateObservation {
    phrase: VerificationPhrase,
    pin: Fingerprint,
}
impl CertificateObservation {
    pub fn phrase(&self) -> &VerificationPhrase {
        &self.phrase
    }
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.pin
    }
    /// Consumes the exact observation whose phrase the native UI displayed.
    /// The returned pin is temporary; persist it only after successful pairing.
    pub fn confirm_phrase(self, matches: bool) -> Result<ConfirmedCertificate> {
        if !matches {
            return Err(RemoteError::new(
                ErrorCode::Unauthorized,
                "Certificate phrase must be confirmed.",
            ));
        }
        Ok(ConfirmedCertificate { pin: self.pin })
    }
}
pub struct ConfirmedCertificate {
    pin: Fingerprint,
}
impl ConfirmedCertificate {
    pub fn client_config(&self) -> Arc<rustls::ClientConfig> {
        pinned_client_config(self.pin.clone())
    }
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.pin
    }
}
#[derive(Debug)]
struct ProbeVerifier(Arc<std::sync::Mutex<Option<Vec<u8>>>>);
impl rustls::client::danger::ServerCertVerifier for ProbeVerifier {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        if end.len() <= 65536 {
            *self.0.lock().unwrap() = Some(end.to_vec());
        }
        Err(rustls::Error::General(
            "certificate probe deliberately aborted".into(),
        ))
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("unconfirmed certificate".into()))
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("unconfirmed certificate".into()))
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
