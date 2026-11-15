use foreplay_harness::HarnessSession;
use foreplay_ir::IrScene;

fn main() {
    let mut session = HarnessSession::new(IrScene::new("DefaultProject"));
    let ticks = session.verify_headless_ticks(60);
    println!(
        "foreplay-harness v0.1.0 [READY] | scene='{}' | verified_headless_ticks={ticks}",
        session.tree.active_scene.name
    );
}
