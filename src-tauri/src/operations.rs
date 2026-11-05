use crate::{app_state::AppState, contracts::*};
use tauri::Emitter;
use tokio_util::sync::CancellationToken;
pub fn start<F, Fut>(
    app: tauri::AppHandle,
    state: AppState,
    work: F,
) -> Result<OperationRef, AppError>
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), AppError>> + Send + 'static,
{
    let id = uuid::Uuid::new_v4().to_string();
    let token = CancellationToken::new();
    {
        let mut operations = state
            .operations
            .lock()
            .map_err(|_| AppError::new("state_error", "Operations are unavailable."))?;
        if operations.len() >= 32 {
            return Err(AppError::new(
                "busy",
                "Too many operations are running. Wait for current work to finish.",
            ));
        }
        operations.insert(id.clone(), token.clone());
    }
    let result = OperationRef {
        operation_id: id.clone(),
    };
    tauri::async_runtime::spawn(async move {
        let task = tauri::async_runtime::spawn(work(token));
        let mut error = match task.await {
            Ok(result) => result.err(),
            Err(_) => Some(AppError::new(
                "operation_failed",
                "The operation stopped unexpectedly. Try again.",
            )),
        };
        if let Some(error) = &mut error {
            error.operation_id = Some(id.clone());
        }
        if let Ok(mut operations) = state.operations.lock() {
            operations.remove(&id);
        }
        let _ = app.emit_to(
            "main",
            "operation_finished",
            OperationFinished {
                operation_id: id,
                error,
            },
        );
    });
    Ok(result)
}
