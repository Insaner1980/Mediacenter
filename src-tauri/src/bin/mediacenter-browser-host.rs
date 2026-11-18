use mediacenter_lib::remote::{browser_bridge, browser_protocol, browser_runtime};
use std::{io, net::Shutdown, path::PathBuf};

fn run() -> io::Result<()> {
    let origin = std::env::args_os().nth(1);
    if !origin
        .as_deref()
        .and_then(|arg| arg.to_str())
        .is_some_and(browser_protocol::valid_origin)
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or(io::ErrorKind::NotFound)?;
    let fixture = directory.join("mediacenter-browser-proof");
    if cfg!(feature = "browser-proof") && fixture.is_dir() {
        let request = browser_bridge::read_message(&mut io::stdin().lock())?;
        let grant = browser_bridge::redeem(&fixture, request)?;
        return browser_bridge::write_message(&mut io::stdout().lock(), &grant);
    }
    let mut stream = browser_bridge::connect_runtime(&directory.join(browser_runtime::DIRECTORY))?;
    let mut upstream = stream.try_clone()?;
    std::thread::spawn(move || {
        let _ = relay(&mut io::stdin().lock(), &mut upstream);
        let _ = upstream.shutdown(Shutdown::Both);
    });
    relay(&mut stream, &mut io::stdout().lock())
}

fn relay(input: &mut impl io::Read, output: &mut impl io::Write) -> io::Result<()> {
    while let Some(frame) = browser_protocol::read_frame(input)? {
        browser_protocol::write_frame(output, &frame)?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    if run().is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
