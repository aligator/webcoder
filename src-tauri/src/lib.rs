use tauri::{DragDropEvent, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub mod native;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            native::get_encoders_native,
            native::probe_native_path,
            native::encode_native
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("create tokio runtime");
            let backend = runtime
                .block_on(native::NativeBackend::new())
                .map_err(std::io::Error::other)?;
            app.manage(backend);

            let window =
                WebviewWindowBuilder::new(&handle, "main", WebviewUrl::App("index.html".into()))
                    .title("Webcoder")
                    .inner_size(1280.0, 900.0)
                    .build()?;

            // The webview swallows HTML5 drag-drop, so the OS drop arrives as a
            // Tauri window DragDrop event. Forward the dropped paths to the
            // frontend as a plain app event it probes with native FFmpeg.
            let emitter = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                    let paths: Vec<String> = paths
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    let _ = emitter.emit("webcoder-files-dropped", paths);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run tauri app");
}
