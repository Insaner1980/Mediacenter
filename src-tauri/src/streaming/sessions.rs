use crate::contracts::ServiceId;
#[derive(Debug, PartialEq)]
pub enum Activation {
    Launch,
    Focus,
    SwitchRequired,
}
pub fn activation(active: Option<ServiceId>, requested: ServiceId) -> Activation {
    match active {
        None => Activation::Launch,
        Some(id) if id == requested => Activation::Focus,
        Some(_) => Activation::SwitchRequired,
    }
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
    match activation(browser.as_ref().and_then(|b| b.session.service_id), service) {
        Activation::Focus => return browser.as_ref().unwrap().focus().await,
        Activation::SwitchRequired => {
            return Err(AppError::new(
                "session_switch_required",
                "Close the current managed service before opening another service.",
            ))
        }
        Activation::Launch => {}
    }
    let home = state.snapshot()?;
    if home.sessions.iter().any(|session| {
        (session.kind == "local-video" || session.kind == "local-audio")
            && !["ended", "error"].contains(&session.phase.as_str())
    }) {
        return Err(AppError::new(
            "session_switch_required",
            "Stop the current local video before opening a service.",
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
        // Ordinary Chrome is not registered as an owned playback session: its
        // windows, streams and lifetime belong to the user, not Mediacenter.
        return super::external::open_hbo(cancel).await;
    }
    let (monitors, computer) = crate::outputs::monitors::discover(&app)?;
    let monitor = crate::outputs::monitors::resolve(&monitors, None, computer.as_deref())?.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn double_click_creates_one_session() {
        assert_eq!(activation(None, ServiceId::Youtube), Activation::Launch);
        assert_eq!(
            activation(Some(ServiceId::Youtube), ServiceId::Youtube),
            Activation::Focus
        );
    }
    #[test]
    fn service_switch_requires_explicit_close() {
        assert_eq!(
            activation(Some(ServiceId::Youtube), ServiceId::Netflix),
            Activation::SwitchRequired
        );
    }
}
