//! Task 17 fixture. Grants a single launch of the synthetic loopback catalog.
use mediacenter_lib::remote::browser_bridge::{Broker, LaunchGrant, LaunchRequest};
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

fn main() -> io::Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(io::ErrorKind::NotFound)?;
    let mut args = std::env::args().skip(1);
    let profile = args.next().ok_or(io::ErrorKind::InvalidInput)?;
    let nonce = args.next().ok_or(io::ErrorKind::InvalidInput)?;
    let port: u16 = args
        .next()
        .and_then(|arg| arg.parse().ok())
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut broker = Broker::bind(&PathBuf::from(runtime).join("mediacenter-browser-proof"))?;
    broker.issue(
        LaunchRequest { profile, nonce },
        LaunchGrant {
            url: format!("http://127.0.0.1:{port}/catalog"),
            session: uuid::Uuid::new_v4().to_string(),
            text: "synthetic café 🦊".into(),
        },
        Duration::from_secs(15),
    );
    println!("ready");
    io::stdout().flush()?;
    broker.serve_once()
}
