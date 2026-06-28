use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[derive(Clone, Default)]
pub struct Readiness(Arc<AtomicBool>);
impl Readiness {
    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::SeqCst);
    }
    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
