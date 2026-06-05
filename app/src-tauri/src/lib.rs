// kild's desktop shell. It hosts the web cockpit in a native window — all logic
// lives in the kild engine (TypeScript), which the frontend talks to over HTTP +
// WebSocket. The only extra responsibility here is process lifecycle: in a release
// build the engine ships as a bundled sidecar binary, and the shell launches it.
// In development the engine is started by `beforeDevCommand` instead.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(not(debug_assertions))]
            {
                use tauri_plugin_shell::ShellExt;
                match app.shell().sidecar("kild-engine") {
                    Ok(cmd) => {
                        let _ = cmd.spawn();
                    }
                    Err(e) => eprintln!("kild: failed to start engine sidecar: {e}"),
                }
            }
            #[cfg(debug_assertions)]
            let _ = &app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
