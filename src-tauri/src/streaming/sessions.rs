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
    use crate::{app_state::now_ms, contracts::AppError, vpn};
    let _session = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
    if state.browser_runtime.session().is_some() {
        return Err(AppError::new(
            "session_switch_required",
            "Close the remote browser tab before opening another service.",
        ));
    }
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
    let publications = crate::events::DeferredPublications::new(&app, &state);
    let mut browser = state
        .browser
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "A service operation is already running."))?;
    if browser
        .as_mut()
        .is_some_and(|current| matches!(current.child.try_wait(), Ok(Some(_))))
    {
        let ended = browser.take().unwrap().session.id;
        publications.publish(|home| {
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
            if vpn::policy::requires_vpn(&home.services, service) {
                let observation =
                    vpn::nordvpn::refresh_deferred(&state, &publications, &cancel).await?;
                vpn::policy::evaluate_launch(true, &observation, now_ms())?;
            }
            let destination = if service == ServiceId::HboMax {
                super::browser::check_chrome_available()?;
                Destination::OrdinaryChrome
            } else {
                let (monitors, computer) = crate::outputs::monitors::discover(&app)?;
                let monitor =
                    crate::outputs::monitors::resolve(&monitors, None, computer.as_deref())?
                        .clone();
                super::browser::prepare_launch(&state.data_dir, service)?;
                Destination::Managed(monitor)
            };
            let permit = vpn::policy::admit_launch(
                &state,
                service,
                &cancel,
                vpn::nordvpn::refresh_deferred_locked(&state, &publications, &cancel),
            )
            .await?;
            Ok((destination, permit))
        },
        |ended| {
            publications.publish(|home| {
                home.sessions.retain(|session| session.id != ended);
                home.outputs.effective = crate::contracts::PlaybackTarget::default();
                home.outputs.state = "idle".into();
                home.outputs.label = "No managed playback".into();
                home.outputs.revision += 1;
            })
        },
    )
    .await?;
    let Some((destination, mut permit)) = prepared else {
        return Ok(());
    };
    permit
        .refresh_if_expired(
            &cancel,
            vpn::nordvpn::refresh_deferred_locked(&state, &publications, &cancel),
        )
        .await?;
    let monitor = match destination {
        Destination::OrdinaryChrome => return super::external::open_hbo(cancel.clone()).await,
        Destination::Managed(monitor) => monitor,
    };
    let mut started = super::browser::launch(&state.data_dir, service, cancel.clone()).await?;
    drop(permit);
    if cancel.is_cancelled() {
        let _ = started.close().await;
        return Err(AppError::new("cancelled", "Service opening was cancelled."));
    }
    let placement = super::placement::initialize_browser(&mut started, &monitor).await;
    let session = started.session.clone();
    *browser = Some(started);
    publications.publish(|home| {
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
    if state
        .browser_runtime
        .session()
        .is_some_and(|s| s.id.as_str() == id)
    {
        return crate::remote::browser_input::focus(state, id).await;
    }
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
    if state
        .browser_runtime
        .session()
        .is_some_and(|s| s.id.as_str() == id)
    {
        return crate::remote::browser_input::close(app, state, id).await;
    }
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
    drop(browser);
    crate::events::publish(app, state, |home| {
        home.sessions.retain(|session| session.id != id)
    })
}
pub async fn reconcile(app: &tauri::AppHandle, state: &crate::app_state::AppState) {
    // Retire disconnected or locally closed extension sessions from the main activity.
    let Ok(_session) = state.session_gate.try_lock() else {
        return;
    };
    let bridged = state.browser_runtime.session().map(|session| session.id);
    let Ok(mut registry) = state.browser.try_lock() else {
        return;
    };
    let native = registry.as_ref().map(|session| session.session.id.as_str());
    let is_live = |id: &str| {
        native == Some(id)
            || bridged
                .as_ref()
                .is_some_and(|session| session.as_str() == id)
    };
    if state.snapshot().is_ok_and(|home| {
        home.sessions
            .iter()
            .any(|s| s.kind == "streaming" && !is_live(&s.id))
    }) {
        let _ = crate::events::publish(app, state, |home| {
            home.sessions
                .retain(|session| session.kind != "streaming" || is_live(&session.id))
        });
    }
    let Some(browser) = registry.as_mut() else {
        return;
    };
    if matches!(browser.child.try_wait(), Ok(Some(_))) && browser.session.phase != "unavailable" {
        browser.session.phase = "unavailable".into();
        let session = browser.session.clone();
        drop(registry);
        let _ = crate::events::publish(app, state, |home| {
            if let Some(current) = home.sessions.iter_mut().find(|s| s.id == session.id) {
                *current = session;
            }
        });
    }
}
