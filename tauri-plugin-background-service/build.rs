const COMMANDS: &[&str] = &["start", "stop", "is_running"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
