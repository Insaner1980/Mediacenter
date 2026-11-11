//! Main-window renderer correlation. This owner lives with AppState, not the listener.
use super::{adapters::checked, protocol::*};
use crate::app_state::{now_ms, AppState};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Emitter;
use tokio::sync::oneshot;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SemanticAction {
    Media {
        #[serde(rename = "mediaKind")]
        media_kind: MediaKind,
        #[serde(rename = "itemId")]
        item_id: Id,
        resume: bool,
    },
    Confirmation {
        #[serde(rename = "confirmationId")]
        confirmation_id: Id,
        confirm: bool,
    },
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererRequest {
    dispatch_id: Id,
    request_id: Id,
    origin: String,
    owner_device_id: Id,
    request: UiRequest,
    expires_at_ms: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmationPresentation {
    dispatch_id: Id,
    request_id: Id,
    renderer_generation: Decimal,
    presentation_revision: Decimal,
    previews: Vec<ConfirmationPreview>,
    expires_at_ms: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererResponse {
    dispatch_id: Id,
    renderer_generation: Decimal,
    context: UiTopic,
    action: Option<SemanticAction>,
    error: Option<ErrorCode>,
}
type AckReceiver = oneshot::Receiver<Result<RendererResponse, RemoteError>>;
type PendingInput = (RendererRequest, AckReceiver, PendingGuard);
struct Pending {
    generation: Decimal,
    deadline: Instant,
    activate: bool,
    sender: oneshot::Sender<Result<RendererResponse, RemoteError>>,
}
#[derive(Default)]
struct Inner {
    generation: u64,
    presentation_revision: u64,
    context: Option<UiTopic>,
    seen: Option<Instant>,
    pending: HashMap<String, Pending>,
}
#[derive(Clone, Default)]
pub struct RendererBridge {
    inner: Arc<Mutex<Inner>>,
}
fn unavailable() -> RemoteError {
    RemoteError::new(
        ErrorCode::RendererUnavailable,
        "The main window is not responding.",
    )
}
fn stale() -> RemoteError {
    RemoteError::new(ErrorCode::StaleState, "Refresh the main window focus.")
}
impl RendererBridge {
    pub fn register(&self) -> Decimal {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        inner.context = None;
        inner.seen = None;
        inner.pending.clear();
        inner.generation.into()
    }
    pub fn update(&self, generation: &Decimal, context: UiTopic) -> Result<(), RemoteError> {
        let mut inner = self.inner.lock().unwrap();
        if generation.value() != inner.generation
            || context.renderer_generation.as_ref() != Some(generation)
            || !context.available
            || context.focus_revision.is_none()
            || context.route.is_none()
        {
            return Err(stale());
        }
        if inner
            .context
            .as_ref()
            .and_then(|c| c.focus_revision.as_ref())
            .is_some_and(|r| r.value() > context.focus_revision.as_ref().unwrap().value())
        {
            return Err(stale());
        }
        inner.context = Some(context);
        inner.seen = Some(Instant::now());
        Ok(())
    }
    pub fn snapshot(&self, revision: u64) -> UiTopic {
        let inner = self.inner.lock().unwrap();
        if inner
            .seen
            .is_some_and(|seen| seen.elapsed() < Duration::from_secs(5))
        {
            if let Some(mut context) = inner.context.clone() {
                context.revision = revision.into();
                return context;
            }
        }
        UiTopic {
            revision: revision.into(),
            available: false,
            renderer_generation: None,
            route: None,
            modal_id: None,
            focus_id: None,
            focus_revision: None,
        }
    }
    fn begin(
        &self,
        owner: &RemotePrincipal,
        request_id: Id,
        origin: &str,
        request: UiRequest,
    ) -> Result<PendingInput, RemoteError> {
        let context = self.snapshot(0);
        if !context.available {
            return Err(unavailable());
        }
        let (generation, revision, modal) = match &request {
            UiRequest::UiNavigate { params } => (
                &params.renderer_generation,
                &params.focus_revision,
                &params.modal_id,
            ),
            UiRequest::UiMoveFocus { params } => (
                &params.renderer_generation,
                &params.focus_revision,
                &params.modal_id,
            ),
            UiRequest::UiActivate { params } => (
                &params.renderer_generation,
                &params.focus_revision,
                &params.modal_id,
            ),
            UiRequest::UiBack { params } => (
                &params.renderer_generation,
                &params.focus_revision,
                &params.modal_id,
            ),
            UiRequest::UiSearch { params } => (
                &params.renderer_generation,
                &params.focus_revision,
                &params.modal_id,
            ),
        };
        if context.renderer_generation.as_ref() != Some(generation)
            || context.focus_revision.as_ref() != Some(revision)
            || &context.modal_id != modal
        {
            return Err(stale());
        }
        let mut inner = self.inner.lock().unwrap();
        if generation.value() != inner.generation {
            return Err(stale());
        }
        if inner.pending.len() >= 32 {
            return Err(RemoteError::new(ErrorCode::Busy, "Too much pending input."));
        }
        let dispatch_id: Id = checked(uuid::Uuid::new_v4().to_string())?;
        let (sender, receiver) = oneshot::channel();
        inner.pending.insert(
            dispatch_id.as_str().into(),
            Pending {
                generation: generation.clone(),
                deadline: Instant::now() + Duration::from_secs(4),
                activate: matches!(
                    request,
                    UiRequest::UiActivate { .. } | UiRequest::UiBack { .. }
                ),
                sender,
            },
        );
        let guard = PendingGuard {
            bridge: self.clone(),
            id: dispatch_id.as_str().into(),
            generation: generation.clone(),
        };
        Ok((
            RendererRequest {
                dispatch_id,
                request_id,
                origin: origin.into(),
                owner_device_id: owner.device_id.clone(),
                request,
                expires_at_ms: now_ms() + 4000,
            },
            receiver,
            guard,
        ))
    }
    pub async fn send(
        &self,
        app: Option<&tauri::AppHandle>,
        owner: &RemotePrincipal,
        request_id: Id,
        origin: &str,
        request: UiRequest,
    ) -> Result<RendererResponse, RemoteError> {
        let (envelope, receiver, guard) = self.begin(owner, request_id, origin, request)?;
        app.ok_or_else(unavailable)?
            .emit_to("main", "remote-ui-request", envelope)
            .map_err(|_| unavailable())?;
        self.wait_for_ack(receiver, &guard.generation).await
    }
    async fn wait_for_ack(
        &self,
        receiver: AckReceiver,
        generation: &Decimal,
    ) -> Result<RendererResponse, RemoteError> {
        match tokio::time::timeout(Duration::from_secs(4), receiver).await {
            Ok(Ok(result)) => result,
            _ => {
                let mut inner = self.inner.lock().unwrap();
                if inner.generation == generation.value() {
                    inner.context = None;
                    inner.seen = None;
                    inner.pending.clear();
                }
                Err(unavailable())
            }
        }
    }
    async fn present(
        &self,
        app: &tauri::AppHandle,
        previews: Vec<ConfirmationPreview>,
        generation: Decimal,
        presentation_revision: u64,
    ) -> Result<(), RemoteError> {
        let (receiver, guard, envelope) = {
            let mut inner = self.inner.lock().unwrap();
            if inner.generation != generation.value() {
                return Err(stale());
            }
            if inner.pending.len() >= 32 {
                return Err(RemoteError::new(ErrorCode::Busy, "Too much pending input."));
            }
            let id: Id = checked(uuid::Uuid::new_v4().to_string())?;
            let (sender, receiver) = oneshot::channel();
            inner.pending.insert(
                id.as_str().into(),
                Pending {
                    generation: generation.clone(),
                    deadline: Instant::now() + Duration::from_secs(4),
                    activate: false,
                    sender,
                },
            );
            let guard = PendingGuard {
                bridge: self.clone(),
                id: id.as_str().into(),
                generation: generation.clone(),
            };
            let envelope = ConfirmationPresentation {
                dispatch_id: id.clone(),
                request_id: id,
                renderer_generation: generation.clone(),
                presentation_revision: presentation_revision.into(),
                previews,
                expires_at_ms: now_ms() + 4000,
            };
            (receiver, guard, envelope)
        };
        app.emit_to("main", "remote-ui-confirmation", envelope)
            .map_err(|_| unavailable())?;
        let response = self.wait_for_ack(receiver, &guard.generation).await?;
        self.update(&generation, response.context)
    }
    fn acknowledge(&self, response: RendererResponse) -> Result<(), RemoteError> {
        let mut inner = self.inner.lock().unwrap();
        if response.renderer_generation.value() != inner.generation {
            return Err(stale());
        }
        let pending = inner
            .pending
            .remove(response.dispatch_id.as_str())
            .ok_or_else(stale)?;
        if pending.generation != response.renderer_generation
            || pending.deadline < Instant::now()
            || response.context.renderer_generation.as_ref() != Some(&pending.generation)
            || response.context.focus_revision.is_none()
            || (!pending.activate && response.action.is_some())
        {
            return Err(stale());
        }
        let result = if let Some(code) = response.error.clone() {
            Err(RemoteError::new(
                code,
                "The main window could not apply this input.",
            ))
        } else {
            Ok(response)
        };
        pending.sender.send(result).map_err(|_| stale())
    }
}
struct PendingGuard {
    bridge: RendererBridge,
    id: String,
    generation: Decimal,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.bridge.inner.lock().unwrap().pending.remove(&self.id);
    }
}
pub(super) fn main_window(label: &str) -> Result<(), RemoteError> {
    if label == "main" {
        Ok(())
    } else {
        Err(RemoteError::new(
            ErrorCode::Forbidden,
            "Main window required.",
        ))
    }
}
pub fn publish(state: &AppState) {
    let stamp = state.event_hub.reserve(Topic::Ui);
    let data = state.renderer.snapshot(stamp.revision);
    state
        .event_hub
        .publish(stamp, |server_epoch, sequence| RemoteEvent::Ui {
            server_epoch,
            sequence,
            data,
        });
}
#[tauri::command]
pub fn register_remote_renderer(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Decimal, RemoteError> {
    main_window(window.label())?;
    let generation = state.renderer.register();
    publish(&state);
    Ok(generation)
}
#[tauri::command]
pub fn update_remote_renderer(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    generation: Decimal,
    context: UiTopic,
) -> Result<(), RemoteError> {
    main_window(window.label())?;
    let before = state.renderer.snapshot(0);
    state.renderer.update(&generation, context)?;
    if before != state.renderer.snapshot(0) {
        publish(&state);
    }
    Ok(())
}
#[tauri::command]
pub fn acknowledge_remote_renderer(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    response: RendererResponse,
) -> Result<(), RemoteError> {
    main_window(window.label())?;
    state.renderer.acknowledge(response)
}

impl super::dispatch::Dispatcher {
    pub(super) fn present_confirmations(&self) {
        let Some(app) = self.services.app().cloned() else {
            return;
        };
        let Ok(registry) = self.services.confirmations.lock() else {
            return;
        };
        let previews = registry.previews(now_ms());
        let Some(generation) = self.services.state.renderer.snapshot(0).renderer_generation else {
            return;
        };
        let revision = {
            let mut inner = self.services.state.renderer.inner.lock().unwrap();
            inner.presentation_revision += 1;
            inner.presentation_revision
        };
        drop(registry);
        let state = self.services.state.clone();
        tauri::async_runtime::spawn(async move {
            // The native/phone operation does not depend on desktop availability.
            // Presentation is applied only when this generation acknowledges it.
            let _ = state
                .renderer
                .present(&app, previews, generation, revision)
                .await;
            publish(&state);
        });
    }
    async fn require_input_unlocked(&self) -> Result<(), RemoteError> {
        #[cfg(test)]
        if let Some(observe) = &self.lock_observation {
            return super::idle::lock_decision(observe());
        }
        if self.services.app().is_none() {
            return Err(unavailable());
        }
        super::idle::require_unlocked().await
    }
    pub async fn renderer_input(
        &self,
        owner: &RemotePrincipal,
        request: RemoteRequest,
        origin: &str,
    ) -> Result<serde_json::Value, RemoteError> {
        Self::authorize(owner, &request)?;
        if request
            .expected
            .as_ref()
            .and_then(|e| e.server_epoch.as_ref())
            != Some(&self.services.state.event_hub.epoch())
        {
            return Err(stale());
        }
        self.require_input_unlocked().await?;
        let ui = match request.command {
            RemoteCommand::UiNavigate(params) => UiRequest::UiNavigate { params },
            RemoteCommand::UiMoveFocus(params) => UiRequest::UiMoveFocus { params },
            RemoteCommand::UiActivate(params) => UiRequest::UiActivate { params },
            RemoteCommand::UiBack(params) => UiRequest::UiBack { params },
            RemoteCommand::UiSearch(params) => UiRequest::UiSearch { params },
            _ => return Err(RemoteError::invalid()),
        };
        let response = match self
            .services
            .state
            .renderer
            .send(self.services.app(), owner, request.id.clone(), origin, ui)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                publish(&self.services.state);
                return Err(error);
            }
        };
        self.services
            .state
            .renderer
            .update(&response.renderer_generation, response.context.clone())?;
        publish(&self.services.state);
        if let Some(action) = response.action {
            self.require_input_unlocked().await?;
            self.semantic_action(owner, action).await?;
        }
        serde_json::to_value(UiAck {
            request_id: request.id,
            renderer_generation: response.renderer_generation,
            focus_revision: response.context.focus_revision.ok_or_else(stale)?,
        })
        .map_err(|_| RemoteError::invalid())
    }
    pub(super) async fn semantic_action(
        &self,
        owner: &RemotePrincipal,
        action: SemanticAction,
    ) -> Result<(), RemoteError> {
        let (method, params) = match action {
            SemanticAction::Media {
                media_kind,
                item_id,
                resume,
            } => (
                "activity.prepare",
                serde_json::json!({"kind":media_kind,"itemId":item_id,"start":if resume {PlaybackStart::Resume}else{PlaybackStart::Beginning}}),
            ),
            SemanticAction::Confirmation {
                confirmation_id,
                confirm,
            } => {
                let pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?
                    .get(&owner.device_id, &confirmation_id, now_ms())?;
                let method = if !confirm {
                    "confirmations.cancel"
                } else {
                    match pending.action {
                        crate::control::confirmations::Action::EndSession(_) => {
                            "settings.endSession"
                        }
                        crate::control::confirmations::Action::Vpn(_) => "vpn.confirmChange",
                        crate::control::confirmations::Action::Admin(action) => {
                            if action.target.is_some() {
                                "devices.revoke"
                            } else {
                                "pairing.begin"
                            }
                        }
                        crate::control::confirmations::Action::Eject(_) => "drives.confirmEject",
                        crate::control::confirmations::Action::Activity { .. } => {
                            "activity.confirm"
                        }
                        crate::control::confirmations::Action::Audio { .. } => "audio.confirm",
                        crate::control::confirmations::Action::Library(action) => match action {
                            crate::control::library::LibraryAction::Add { .. } => {
                                "folders.confirmAdd"
                            }
                            crate::control::library::LibraryAction::Reattach { .. } => {
                                "sources.confirmReattachment"
                            }
                            crate::control::library::LibraryAction::Remove { .. } => {
                                "sources.confirmRemove"
                            }
                        },
                    }
                };
                (
                    method,
                    serde_json::json!({"confirmationId":confirmation_id}),
                )
            }
        };
        let request=parse_request(&serde_json::json!({"v":1,"id":uuid::Uuid::new_v4().to_string(),"method":method,"params":params,"expected":{"serverEpoch":self.services.state.event_hub.epoch()}}).to_string())?;
        // Re-enter domain authorization with the original authenticated actor.
        Box::pin(self.dispatch(owner, request)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> (RendererBridge, Decimal) {
        let bridge = RendererBridge::default();
        let generation = bridge.register();
        bridge.update(&generation,checked(serde_json::json!({"revision":"0","available":true,"rendererGeneration":generation,"focusRevision":"1","route":"videos","focusId":"video-a"})).unwrap()).unwrap();
        (bridge, generation)
    }
    fn owner(name: &str) -> RemotePrincipal {
        checked(serde_json::json!({"deviceId":name,"scopes":["navigation"]})).unwrap()
    }
    fn input(generation: &Decimal) -> UiRequest {
        checked(serde_json::json!({"method":"ui.activate","params":{"rendererGeneration":generation,"focusRevision":"1","focusId":"video-a"}})).unwrap()
    }
    fn response(envelope: &RendererRequest, bridge: &RendererBridge) -> RendererResponse {
        RendererResponse {
            dispatch_id: envelope.dispatch_id.clone(),
            renderer_generation: bridge.snapshot(0).renderer_generation.unwrap(),
            context: bridge.snapshot(0),
            action: None,
            error: None,
        }
    }
    #[tokio::test]
    async fn task11_same_wire_ids_are_isolated_until_each_renderer_ack() {
        let (bridge, generation) = ready();
        let (a, mut ar, _ag) = bridge
            .begin(
                &owner("a"),
                checked("same-id").unwrap(),
                "connection-a",
                input(&generation),
            )
            .unwrap();
        let (b, mut br, _bg) = bridge
            .begin(
                &owner("b"),
                checked("same-id").unwrap(),
                "connection-b",
                input(&generation),
            )
            .unwrap();
        assert_ne!(a.dispatch_id, b.dispatch_id);
        assert_ne!(a.origin, b.origin);
        assert_eq!(a.owner_device_id, owner("a").device_id);
        assert!(matches!(
            ar.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        bridge.acknowledge(response(&b, &bridge)).unwrap();
        assert!(br.try_recv().unwrap().is_ok());
        assert!(matches!(
            ar.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        bridge.acknowledge(response(&a, &bridge)).unwrap();
        assert!(ar.await.unwrap().is_ok());
    }
    #[tokio::test]
    async fn task11_reload_and_cancellation_invalidate_pending_input() {
        let (bridge, generation) = ready();
        let (a, ar, guard) = bridge
            .begin(
                &owner("a"),
                checked("id").unwrap(),
                "connection",
                input(&generation),
            )
            .unwrap();
        let old = response(&a, &bridge);
        let replacement = bridge.register();
        assert!(replacement.value() > generation.value());
        assert!(!bridge.snapshot(0).available);
        assert!(ar.await.is_err());
        assert_eq!(
            bridge.acknowledge(old).unwrap_err().code,
            ErrorCode::StaleState
        );
        drop(guard);
        let (bridge, generation) = ready();
        let (a, ar, guard) = bridge
            .begin(
                &owner("a"),
                checked("id").unwrap(),
                "connection",
                input(&generation),
            )
            .unwrap();
        let reply = response(&a, &bridge);
        drop(guard);
        assert!(ar.await.is_err());
        assert_eq!(
            bridge.acknowledge(reply).unwrap_err().code,
            ErrorCode::StaleState
        );
    }
    #[tokio::test]
    async fn task11_expired_ack_cannot_authorize_an_action() {
        let (bridge, generation) = ready();
        let (a, ar, _guard) = bridge
            .begin(
                &owner("a"),
                checked("id").unwrap(),
                "connection",
                input(&generation),
            )
            .unwrap();
        bridge
            .inner
            .lock()
            .unwrap()
            .pending
            .get_mut(a.dispatch_id.as_str())
            .unwrap()
            .deadline = Instant::now() - Duration::from_secs(1);
        let mut reply = response(&a, &bridge);
        reply.action = Some(SemanticAction::Media {
            media_kind: MediaKind::Video,
            item_id: checked("movie").unwrap(),
            resume: false,
        });
        assert_eq!(
            bridge.acknowledge(reply).unwrap_err().code,
            ErrorCode::StaleState
        );
        assert!(ar.await.is_err());
    }
    #[test]
    fn task11_stale_renderer_context_and_lost_heartbeat_are_not_ready() {
        let (bridge, generation) = ready();
        let mut stale_input = input(&generation);
        if let UiRequest::UiActivate { params } = &mut stale_input {
            params.focus_revision = 0.into();
        }
        assert!(
            matches!(bridge.begin(&owner("a"),checked("id").unwrap(),"connection",stale_input),Err(error) if error.code==ErrorCode::StaleState)
        );
        bridge.inner.lock().unwrap().seen = Some(Instant::now() - Duration::from_secs(6));
        assert!(!bridge.snapshot(0).available);
    }
    #[tokio::test]
    async fn task11_navigation_scope_cannot_play_media_or_approve_another_owner() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services =
            crate::control::ControlServices::native(state, checked("server").unwrap()).unwrap();
        let dispatcher = super::super::dispatch::Dispatcher::new(services.clone());
        let result = dispatcher
            .semantic_action(
                &owner("a"),
                SemanticAction::Media {
                    media_kind: MediaKind::Video,
                    item_id: checked("movie").unwrap(),
                    resume: false,
                },
            )
            .await;
        assert_eq!(result.unwrap_err().code, ErrorCode::Forbidden);
        let preview:ConfirmationPreview=checked(serde_json::json!({"confirmationId":"confirm","ownerDeviceId":"b","action":"replace_activity","affectedLabels":["Movie"],"expected":{"serverEpoch":services.state.event_hub.epoch()},"expiresInMs":60000,"confirmMethod":"activity.confirm"})).unwrap();
        services
            .confirmations
            .lock()
            .unwrap()
            .insert(
                crate::control::confirmations::Pending {
                    preview,
                    action: crate::control::confirmations::Action::Activity {
                        target: checked(serde_json::json!({"kind":"video","itemId":"movie"}))
                            .unwrap(),
                        media: Default::default(),
                    },
                    observed: Default::default(),
                    request: checked("prepare").unwrap(),
                    expires_at: now_ms() + 60000,
                    operation: None,
                },
                now_ms(),
            )
            .unwrap();
        assert_eq!(
            dispatcher
                .semantic_action(
                    &owner("a"),
                    SemanticAction::Confirmation {
                        confirmation_id: checked("confirm").unwrap(),
                        confirm: true
                    }
                )
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert!(services
            .confirmations
            .lock()
            .unwrap()
            .get(
                &owner("b").device_id,
                &checked("confirm").unwrap(),
                now_ms()
            )
            .unwrap()
            .operation
            .is_none());
    }
    #[tokio::test(start_paused = true)]
    async fn task11_missing_renderer_ack_times_out_without_success_or_late_effect() {
        let (bridge, generation) = ready();
        let (envelope, receiver, guard) = bridge
            .begin(
                &owner("a"),
                checked("id").unwrap(),
                "connection",
                input(&generation),
            )
            .unwrap();
        let late = response(&envelope, &bridge);
        assert_eq!(
            bridge
                .wait_for_ack(receiver, &generation)
                .await
                .err()
                .unwrap()
                .code,
            ErrorCode::RendererUnavailable
        );
        assert!(!bridge.snapshot(0).available);
        assert_eq!(
            bridge.acknowledge(late).unwrap_err().code,
            ErrorCode::StaleState
        );
        drop(guard);
    }
    #[test]
    fn task11_bridge_registration_is_shared_across_control_services_and_main_only() {
        assert!(main_window("main").is_ok());
        assert_eq!(
            main_window("presentation").unwrap_err().code,
            ErrorCode::Forbidden
        );
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let generation = state.renderer.register();
        state.renderer.update(&generation,checked(serde_json::json!({"revision":"0","available":true,"rendererGeneration":generation,"focusRevision":"1","route":"home"})).unwrap()).unwrap();
        let first =
            crate::control::ControlServices::native(state.clone(), checked("server").unwrap())
                .unwrap();
        let second =
            crate::control::ControlServices::native(state, checked("server").unwrap()).unwrap();
        assert_eq!(
            first.state.renderer.snapshot(0),
            second.state.renderer.snapshot(0)
        );
        assert!(second.state.renderer.snapshot(0).available);
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    #[tokio::test]
    async fn task20_locked_and_unobserved_desktop_refuse_renderer_input_before_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services =
            crate::control::ControlServices::native(state.clone(), checked("server").unwrap())
                .unwrap();
        let mut d = super::super::dispatch::Dispatcher::new(services);
        let owner =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","navigation"]}))
                .unwrap();
        for (observation, expected) in [
            (Some(true), ErrorCode::DesktopLocked),
            (None, ErrorCode::Unavailable),
        ] {
            d.lock_observation = Some(Arc::new(move || observation));
            let request=parse_request(&serde_json::json!({"v":1,"id":"input","method":"ui.navigate","params":{"rendererGeneration":"1","focusRevision":"1","route":"videos"},"expected":{"serverEpoch":state.event_hub.epoch()}}).to_string()).unwrap();
            assert_eq!(
                d.renderer_input(&owner, request, "fixture")
                    .await
                    .unwrap_err()
                    .code,
                expected
            );
            assert!(state.renderer.inner.lock().unwrap().pending.is_empty());
            assert!(state.operations.lock().unwrap().is_empty());
        }
    }
}
