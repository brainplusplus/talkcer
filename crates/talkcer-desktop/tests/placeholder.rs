//! Task 1 keeps the desktop crate in the workspace without Tauri features.

#[test]
fn desktop_points_at_the_local_api() {
    assert_eq!(talkcer_desktop::VERSION, "0.1.0");
    assert_eq!(talkcer_desktop::server_bind(), "127.0.0.1:8787");
    assert_eq!(talkcer_desktop::DEFAULT_API, "http://127.0.0.1:8787/api");
}
