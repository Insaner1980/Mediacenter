use mediacenter_lib::remote::{browser_bridge, browser_protocol};
use std::{io, path::PathBuf};

fn run() -> io::Result<()> {
    let origin = std::env::args_os().nth(1);
    if !origin
        .as_deref()
        .and_then(|arg| arg.to_str())
        .is_some_and(browser_protocol::valid_origin)
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let request = browser_bridge::read_message(&mut io::stdin().lock())?;
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or(io::ErrorKind::NotFound)?
        .join("mediacenter-browser-proof");
    let grant = browser_bridge::redeem(&directory, request)?;
    browser_bridge::write_message(&mut io::stdout().lock(), &grant)
}

fn main() -> std::process::ExitCode {
    if run().is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
