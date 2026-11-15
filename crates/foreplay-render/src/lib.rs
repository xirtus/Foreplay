//! `RenderingServer` RID tags and frame-graph resource handles.

use foreplay_ir::Rid;

#[derive(Debug)]
pub enum MeshTag {}
#[derive(Debug)]
pub enum MaterialTag {}
#[derive(Debug)]
pub enum ViewportTag {}

pub type MeshRid = Rid<MeshTag>;
pub type MaterialRid = Rid<MaterialTag>;
pub type ViewportRid = Rid<ViewportTag>;

/// Minimal Phase 0 `RenderingServer` contract (Track B Carbon High owns `wgpu` implementation).
#[derive(Debug, Default)]
pub struct RenderingServer {
    next_raw_rid: u64,
}

impl RenderingServer {
    pub fn new() -> Self {
        Self { next_raw_rid: 1 }
    }

    pub fn create_viewport(&mut self) -> ViewportRid {
        let id = self.next_raw_rid;
        self.next_raw_rid += 1;
        Rid::from_raw(id)
    }
}
