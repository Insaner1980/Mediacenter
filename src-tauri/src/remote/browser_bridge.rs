//! Local authentication and one-use grants for the browser feasibility bridge.
use super::browser_protocol::{read_frame, write_frame};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

const SECRET_BYTES: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    pub profile: String,
    pub nonce: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BrokerRequest {
    secret: [u8; SECRET_BYTES],
    launch: LaunchRequest,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchGrant {
    pub url: String,
    pub session: String,
    pub text: String,
}

pub struct Broker {
    directory: PathBuf,
    listener: UnixListener,
    owner: u32,
    secret: [u8; SECRET_BYTES],
    pending: Option<(LaunchRequest, LaunchGrant, Instant)>,
}

impl Broker {
    pub fn bind(directory: &Path) -> io::Result<Self> {
        DirBuilder::new().mode(0o700).create(directory)?;
        let result = Self::bind_private(directory);
        if result.is_err() {
            remove_broker_files(directory);
        }
        result
    }

    fn bind_private(directory: &Path) -> io::Result<Self> {
        let owner = owner();
        check_directory(directory, owner)?;
        let mut secret = [0; SECRET_BYTES];
        getrandom::fill(&mut secret).map_err(|_| io::Error::other("broker entropy unavailable"))?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("secret"))?
            .write_all(&secret)?;
        let socket = directory.join("broker.sock");
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            directory: directory.into(),
            listener,
            owner,
            secret,
            pending: None,
        })
    }

    pub fn issue(&mut self, request: LaunchRequest, grant: LaunchGrant, lifetime: Duration) {
        self.pending = Some((request, grant, Instant::now() + lifetime));
    }

    pub fn serve_once(&mut self) -> io::Result<()> {
        let (mut stream, _) = self.listener.accept()?;
        check_peer(&stream, self.owner)?;
        configure(&stream)?;
        let request: BrokerRequest = read_message(&mut stream)?;
        let valid = bool::from(self.secret.ct_eq(&request.secret))
            && self.pending.as_ref().is_some_and(|(expected, _, expiry)| {
                Instant::now() < *expiry
                    && request.launch.profile == expected.profile
                    && bool::from(
                        request
                            .launch
                            .nonce
                            .as_bytes()
                            .ct_eq(expected.nonce.as_bytes()),
                    )
            });
        if !valid {
            return Err(denied());
        }
        let (_, grant, _) = self.pending.take().ok_or_else(denied)?;
        write_message(&mut stream, &grant)
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        remove_broker_files(&self.directory);
    }
}

fn remove_broker_files(directory: &Path) {
    let _ = fs::remove_file(directory.join("broker.sock"));
    let _ = fs::remove_file(directory.join("secret"));
    let _ = fs::remove_dir(directory);
}

pub fn redeem(directory: &Path, launch: LaunchRequest) -> io::Result<LaunchGrant> {
    let owner = owner();
    check_directory(directory, owner)?;
    let secret_file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(directory.join("secret"))?;
    let secret = read_secret(secret_file, owner)?;
    let socket = directory.join("broker.sock");
    let metadata = fs::symlink_metadata(&socket)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != owner
        || metadata.mode() & 0o777 != 0o600
    {
        return Err(denied());
    }
    let mut stream = UnixStream::connect(socket)?;
    check_peer(&stream, owner)?;
    configure(&stream)?;
    write_message(&mut stream, &BrokerRequest { secret, launch })?;
    read_message(&mut stream)
}

fn owner() -> u32 {
    // SAFETY: geteuid has no arguments and no memory preconditions.
    unsafe { nix::libc::geteuid() }
}

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "browser broker authorization failed",
    )
}

fn check_peer(stream: &UnixStream, owner: u32) -> io::Result<()> {
    if getsockopt(stream, PeerCredentials)?.uid() != owner {
        return Err(denied());
    }
    Ok(())
}

fn check_directory(directory: &Path, owner: u32) -> io::Result<()> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.uid() != owner || metadata.mode() & 0o777 != 0o700 {
        return Err(denied());
    }
    Ok(())
}

fn read_secret(mut file: File, owner: u32) -> io::Result<[u8; SECRET_BYTES]> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() != SECRET_BYTES as u64
    {
        return Err(denied());
    }
    let mut secret = [0; SECRET_BYTES];
    file.read_exact(&mut secret)?;
    Ok(secret)
}

fn configure(stream: &UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))
}

pub fn read_message<T: DeserializeOwned>(input: &mut impl Read) -> io::Result<T> {
    let frame = read_frame(input)?.ok_or(io::ErrorKind::UnexpectedEof)?;
    serde_json::from_slice(&frame)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid browser message"))
}

pub fn write_message(output: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let frame = serde_json::to_vec(value)
        .map_err(|_| io::Error::other("could not encode browser message"))?;
    write_frame(output, &frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(nonce: &str) -> LaunchRequest {
        LaunchRequest {
            profile: "test-profile".into(),
            nonce: nonce.into(),
        }
    }

    #[test]
    fn rejects_wrong_peer_uid() {
        let (stream, _) = UnixStream::pair().unwrap();
        assert!(check_peer(&stream, owner()).is_ok());
        assert_eq!(
            check_peer(&stream, owner().wrapping_add(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn protects_socket_and_consumes_only_authenticated_current_grant() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("broker");
        let mut broker = Broker::bind(&directory).unwrap();
        assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o777, 0o700);
        assert_eq!(
            fs::metadata(directory.join("broker.sock")).unwrap().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.join("secret")).unwrap().mode() & 0o777,
            0o600
        );
        let grant = || LaunchGrant {
            url: "http://127.0.0.1:41739/catalog".into(),
            session: "session".into(),
            text: "synthetic".into(),
        };
        broker.issue(request("expired"), grant(), Duration::ZERO);
        std::thread::scope(|scope| {
            let server = scope.spawn(move || {
                assert_eq!(
                    broker.serve_once().unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                );
                broker.issue(request("fresh"), grant(), Duration::from_secs(5));
                assert_eq!(
                    broker.serve_once().unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                );
                assert_eq!(
                    broker.serve_once().unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                );
                broker.serve_once().unwrap();
                assert_eq!(
                    broker.serve_once().unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                );
            });
            assert!(redeem(&directory, request("expired")).is_err());
            let mut foreign = UnixStream::connect(directory.join("broker.sock")).unwrap();
            write_message(
                &mut foreign,
                &BrokerRequest {
                    secret: [0; SECRET_BYTES],
                    launch: request("fresh"),
                },
            )
            .unwrap();
            assert!(read_message::<LaunchGrant>(&mut foreign).is_err());
            assert!(redeem(&directory, request("stale")).is_err());
            assert_eq!(
                redeem(&directory, request("fresh")).unwrap().session,
                "session"
            );
            assert!(redeem(&directory, request("fresh")).is_err());
            server.join().unwrap();
        });
        assert!(!directory.exists());
    }
}
