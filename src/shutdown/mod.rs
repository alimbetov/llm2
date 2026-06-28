use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct ShutdownCoordinator {
    pub stop_accepting: CancellationToken,
    pub stop_claiming: CancellationToken,
    active: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}
pub struct ActiveGuard {
    c: ShutdownCoordinator,
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        if self.c.active.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.c.notify.notify_waiters();
        }
    }
}
impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            stop_accepting: CancellationToken::new(),
            stop_claiming: CancellationToken::new(),
            active: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }
    pub fn enter(&self) -> ActiveGuard {
        self.active.fetch_add(1, Ordering::SeqCst);
        ActiveGuard { c: self.clone() }
    }
    pub async fn drain(&self, timeout: std::time::Duration) -> bool {
        self.stop_accepting.cancel();
        self.stop_claiming.cancel();
        if self.active.load(Ordering::SeqCst) == 0 {
            return true;
        }
        tokio::time::timeout(timeout, self.notify.notified())
            .await
            .is_ok()
    }
}
impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
