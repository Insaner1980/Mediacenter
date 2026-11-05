use crate::contracts::ServiceId;
enum Destination {
    OrdinaryChrome,
    Managed(crate::contracts::MonitorDescriptor),
}
pub async fn open(
    app: tauri::AppHandle,
    state: crate::app_state::AppState,
    service: ServiceId,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::contracts::AppError> {
    use crate::{app_state::now_ms, contracts::AppError, events, vpn};
    let _session = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
    if crate::photos::presenter::snapshot(&state)
        .await
        .session_id
        .is_some()
    {
        return Err(AppError::new(
            "session_switch_required",
            "Stop the slideshow before opening a streaming service.",
        ));
    }
    let mut browser = state
        .browser
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "A service operation is already running."))?;
    if browser
        .as_mut()
        .is_some_and(|current| matches!(current.child.try_wait(), Ok(Some(_))))
    {
        let ended = browser.take().unwrap().session.id;
        events::publish(&app, &state, |home| {
            home.sessions.retain(|session| session.id != ended);
        })?;
    }
    let prepared = super::switching::prepare(
        &mut *browser,
        service,
        &cancel,
        async {
            let home = state.snapshot()?;
            if home.sessions.iter().any(|session| {
                (session.kind == "local-video" || session.kind == "local-audio")
                    && !["ended", "error"].contains(&session.phase.as_str())
            }) {
                return Err(AppError::new(
                    "session_switch_required",
                    "Stop local playback before opening a service.",
                ));
            }
            let required = home
                .services
                .iter()
                .find(|entry| entry.id == service)
                .is_none_or(|entry| entry.vpn_required);
            let observation = vpn::nordvpn::refresh(&app, &state).await?;
            vpn::policy::evaluate_launch(required, &observation, now_ms())?;
            if service == ServiceId::HboMax {
                super::browser::check_chrome_available()?;
                return Ok(Destination::OrdinaryChrome);
            }
            let (monitors, computer) = crate::outputs::monitors::discover(&app)?;
            let monitor =
                crate::outputs::monitors::resolve(&monitors, None, computer.as_deref())?.clone();
            super::browser::prepare_launch(&state.data_dir, service)?;
            Ok(Destination::Managed(monitor))
        },
        |ended| {
            events::publish(&app, &state, |home| {
                home.sessions.retain(|session| session.id != ended);
                home.outputs.effective = crate::contracts::PlaybackTarget::default();
                home.outputs.state = "idle".into();
                home.outputs.label = "No managed playback".into();
                home.outputs.revision += 1;
            })
        },
    )
    .await?;
    let monitor = match prepared {
        None => return Ok(()),
        Some(Destination::OrdinaryChrome) => return super::external::open_hbo(cancel).await,
        Some(Destination::Managed(monitor)) => monitor,
    };
    let mut started = super::browser::launch(&state.data_dir, service, cancel.clone()).await?;
    if cancel.is_cancelled() {
        let _ = started.close().await;
        return Err(AppError::new("cancelled", "Service opening was cancelled."));
    }
    let placement = super::placement::initialize_browser(&mut started, &monitor).await;
    let session = started.session.clone();
    *browser = Some(started);
    events::publish(&app, &state, |home| {
        home.sessions.retain(|session| session.kind != "streaming");
        home.outputs.effective = session.target.clone();
        home.outputs.state = if placement.is_ok() {
            "partial"
        } else {
            "unavailable"
        }
        .into();
        home.outputs.label = match placement {
            Ok(()) => format!("Service on {}; audio not yet confirmed", monitor.name),
            Err(error) => error.message,
        };
        home.outputs.revision += 1;
        home.sessions.push(session);
    })
}
pub async fn focus(
    state: &crate::app_state::AppState,
    id: &str,
) -> Result<(), crate::contracts::AppError> {
    let browser = state.browser.lock().await;
    let current = browser
        .as_ref()
        .filter(|b| b.session.id == id)
        .ok_or_else(|| {
            crate::contracts::AppError::new(
                "session_missing",
                "This service session is unavailable.",
            )
        })?;
    current.focus().await
}
pub async fn close(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
    id: &str,
) -> Result<(), crate::contracts::AppError> {
    let mut browser = state.browser.lock().await;
    let current = browser
        .as_mut()
        .filter(|b| b.session.id == id)
        .ok_or_else(|| {
            crate::contracts::AppError::new(
                "session_missing",
                "This service session is unavailable.",
            )
        })?;
    if current.child.try_wait()?.is_none() {
        current.close().await?;
    }
    *browser = None;
    crate::events::publish(app, state, |home| {
        home.sessions.retain(|session| session.id != id)
    })
}
pub async fn reconcile(app: &tauri::AppHandle, state: &crate::app_state::AppState) {
    let Ok(mut registry) = state.browser.try_lock() else {
        return;
    };
    let Some(browser) = registry.as_mut() else {
        return;
    };
    if matches!(browser.child.try_wait(), Ok(Some(_))) && browser.session.phase != "unavailable" {
        browser.session.phase = "unavailable".into();
        let session = browser.session.clone();
        let _ = crate::events::publish(app, state, |home| {
            if let Some(current) = home.sessions.iter_mut().find(|s| s.id == session.id) {
                *current = session;
            }
        });
    }
}
