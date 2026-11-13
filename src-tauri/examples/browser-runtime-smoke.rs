//! Drives the live Native Messaging broker in a disposable runtime and profile.
use mediacenter_lib::{
    contracts::ServiceId,
    remote::{browser_runtime::Runtime, protocol::Id},
};
use std::{io, path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let runtime = Runtime::default();
    runtime.start(&directory)?;
    println!("listening");
    let cancel = CancellationToken::new();
    if let Some(data) = std::env::args_os().nth(1) {
        let data = PathBuf::from(data);
        assert!(runtime.profile(ServiceId::Youtube, "auto").is_err());
        runtime
            .prepare_profile(&data, ServiceId::Youtube, "auto", &cancel)
            .await?;
        println!("managed profile connected from cold start");
        return Ok(());
    }
    let profile = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(profile) = runtime.profile(ServiceId::Youtube, "auto") {
                return profile;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await?;
    let owner: Id = serde_json::from_value(serde_json::json!("smoke-phone"))?;
    let profile = runtime
        .prepare_profile(&directory, ServiceId::Youtube, &profile, &cancel)
        .await?;
    let session = runtime
        .launch(owner, ServiceId::Youtube, &profile, &cancel)
        .await?;
    let target = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Some(target) = runtime.session().filter(|s| s.ready && s.id == session.id) {
                // Let Chrome's visible debugger banner and the initial document settle
                // before choosing the generation used by this single submitted input.
                tokio::time::sleep(Duration::from_millis(600)).await;
                if runtime
                    .session()
                    .is_some_and(|current| current.ready && current.generation == target.generation)
                {
                    return target;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        }
    })
    .await?;
    runtime.command(&target, serde_json::json!({"type":"input","session":target.id,"generation":target.generation,"input":{"type":"text","text":"synthetic café 🦊"}}), &cancel).await?;
    println!("input submitted");
    tokio::task::spawn_blocking(|| {
        let _ = io::stdin().read_line(&mut String::new());
    })
    .await?;
    runtime
        .command(
            &target,
            serde_json::json!({"type":"close","session":target.id,"generation":target.generation}),
            &cancel,
        )
        .await?;
    println!("closed");
    Ok(())
}
