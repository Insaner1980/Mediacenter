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
    let mut browser = state
        .browser
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "A service operation is already running."))?;
    if browser
        .as_mut()
        .is_some_and(|current| matches!(current.child.try_wait(), Ok(Some(_))))
    {
        *browser = None;
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
        session.kind == "local-video" && !["ended", "error"].contains(&session.phase.as_str())
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
    let mut started = super::browser::launch(&state.data_dir, service, cancel.clone()).await?;
    if cancel.is_cancelled() {
        let _ = started.close().await;
        return Err(AppError::new("cancelled", "Service opening was cancelled."));
    }
    let session = started.session.clone();
    *browser = Some(started);
    events::publish(&app, &state, |home| {
        home.sessions.retain(|session| session.kind != "streaming");
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
    current.close().await?;
    *browser = None;
    crate::events::publish(app, state, |home| {
        home.sessions.retain(|session| session.id != id)
    })
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
