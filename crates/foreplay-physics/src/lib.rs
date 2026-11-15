//! Hotswappable `PhysicsServer3D` supporting live A/B switching (e.g. Rapier vs. Skate 2 / Skate 3 physics).

use foreplay_ir::PhysicsBackend;

#[derive(Debug)]
pub struct PhysicsServer3D {
    active: Box<dyn PhysicsBackend>,
}

impl PhysicsServer3D {
    pub fn new(initial: Box<dyn PhysicsBackend>) -> Self {
        Self { active: initial }
    }

    /// Live A/B hotswap of physics implementation mid-session (`skate2-rust-port` workflow).
    pub fn hotswap_backend(&mut self, next: Box<dyn PhysicsBackend>) -> Box<dyn PhysicsBackend> {
        std::mem::replace(&mut self.active, next)
    }

    pub fn active_backend_id(&self) -> &'static str {
        self.active.backend_id()
    }

    pub fn step(&mut self, dt_seconds: f32) {
        self.active.step(dt_seconds);
    }
}
