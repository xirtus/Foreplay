use foreplay_editor::{EditorWorkspace, ForeplayEditorApp};

fn main() {
    let app = ForeplayEditorApp::new("ForeplayStarter");
    assert_eq!(app.active_workspace, EditorWorkspace::Workspace3D);
    println!(
        "Foreplay Game Engine Editor v0.1.0 (Phase 0 Bootstrap) | workspace={:?} | project='{}'",
        app.active_workspace, app.session.tree.active_scene.name
    );
}
