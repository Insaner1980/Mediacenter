use crate::{
    contracts::{AppError, MonitorDescriptor},
    outputs::x11::WindowInfo,
};
use std::future::Future;
trait ComputerWindow {
    fn observe(&self) -> impl Future<Output = Result<WindowInfo, AppError>> + Send;
    fn place(
        &self,
        monitor: &MonitorDescriptor,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}
async fn initialize(
    window: &impl ComputerWindow,
    monitor: &MonitorDescriptor,
) -> Result<String, AppError> {
    window.observe().await?;
    window.place(monitor).await?;
    let observed = window.observe().await?;
    if !crate::outputs::monitors::reachable(
        monitor,
        observed.x,
        observed.y,
        observed.width,
        observed.height,
    ) {
        return Err(AppError::new("placement_unconfirmed","The service window is open, but placement on the computer was not confirmed. Use its window controls to bring it back."));
    }
    Ok(monitor.id.clone())
}
impl ComputerWindow for super::browser::BrowserSession {
    async fn observe(&self) -> Result<WindowInfo, AppError> {
        self.verify().await
    }
    async fn place(&self, monitor: &MonitorDescriptor) -> Result<(), AppError> {
        crate::outputs::coordinator::place_browser_on_computer(self, monitor)
            .await
            .map(|_| ())
    }
}
pub async fn initialize_browser(
    browser: &mut super::browser::BrowserSession,
    monitor: &MonitorDescriptor,
) -> Result<(), AppError> {
    match initialize(browser, monitor).await {
        Ok(id) => {
            browser.session.target.monitor_id = Some(id);
            browser.session.phase = "ready".into();
            Ok(())
        }
        Err(error) => {
            browser.session.target.monitor_id = None;
            browser.session.phase = "manual placement required".into();
            Err(error)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Bounds;
    use std::sync::Mutex;
    struct Desktop {
        current: Mutex<WindowInfo>,
        requests: Mutex<usize>,
        denied: bool,
    }
    impl ComputerWindow for Desktop {
        async fn observe(&self) -> Result<WindowInfo, AppError> {
            Ok(self.current.lock().unwrap().clone())
        }
        async fn place(&self, m: &MonitorDescriptor) -> Result<(), AppError> {
            *self.requests.lock().unwrap() += 1;
            if !self.denied {
                let mut w = self.current.lock().unwrap();
                w.x = m.bounds.x;
                w.y = m.bounds.y;
                w.width = 800;
                w.height = 600;
            }
            Ok(())
        }
    }
    fn fixture(denied: bool) -> (Desktop, MonitorDescriptor) {
        (
            Desktop {
                current: Mutex::new(WindowInfo {
                    id: "0x10".into(),
                    pid: 20,
                    x: 2000,
                    y: 0,
                    width: 1280,
                    height: 800,
                }),
                requests: Mutex::new(0),
                denied,
            },
            MonitorDescriptor {
                id: "computer".into(),
                name: "Computer".into(),
                connector: "eDP-1".into(),
                primary: false,
                connected: true,
                scale_factor: 1.,
                bounds: Bounds {
                    x: -1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
            },
        )
    }
    #[tokio::test]
    async fn restored_tv_window_is_moved_and_verified_on_computer() {
        let (desktop, monitor) = fixture(false);
        assert_eq!(initialize(&desktop, &monitor).await.unwrap(), "computer");
        assert_eq!(*desktop.requests.lock().unwrap(), 1);
        let w = desktop.observe().await.unwrap();
        assert!(crate::outputs::monitors::reachable(
            &monitor, w.x, w.y, w.width, w.height
        ));
    }
    #[tokio::test]
    async fn ignored_placement_is_not_reported_as_ready() {
        let (desktop, monitor) = fixture(true);
        assert_eq!(
            initialize(&desktop, &monitor).await.unwrap_err().code,
            "placement_unconfirmed"
        );
    }
}
