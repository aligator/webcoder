// Thin bridge between the Yew/WASM frontend and the Tauri backend.
// Every function returns a JSON *string* so the Rust side can deserialize it
// straight into typed structs with serde. All work is done through Tauri
// `invoke` commands and the dialog/event plugins — there is no HTTP surface.

function tauriDialog() {
  return window.__TAURI__ && window.__TAURI__.dialog;
}

function tauriInvoke() {
  return window.__TAURI__.core.invoke;
}

// Subscribe to the desktop shell's OS drag-drop. `callback` receives an array
// of absolute file paths. The Rust side catches the window DragDrop event and
// re-emits the paths as the plain app event below.
export function listenNativeDrop(callback) {
  const event = window.__TAURI__ && window.__TAURI__.event;
  if (!event || !event.listen) return;
  event.listen("webcoder-files-dropped", (e) => {
    const paths = e && e.payload;
    if (Array.isArray(paths) && paths.length) callback(paths);
  });
}

export async function getEncoders() {
  return JSON.stringify(await tauriInvoke()("get_encoders_native"));
}

export async function pickNativeFiles() {
  const dialog = tauriDialog();
  if (!dialog || !dialog.open) return "[]";
  const picked = await dialog.open({ multiple: true, directory: false });
  if (!picked) return "[]";
  return JSON.stringify(Array.isArray(picked) ? picked : [picked]);
}

export async function probeNativePath(path) {
  return JSON.stringify(await tauriInvoke()("probe_native_path", { path }));
}

// Runs the encode for a previously-probed job. settingsJson/tracksJson are
// JSON strings produced by serde on the Rust side; forwarded verbatim.
export async function runEncode(jobId, settingsJson, tracksJson, outputDir, overwrite) {
  return JSON.stringify(await tauriInvoke()("encode_native", {
    jobId,
    settings: JSON.parse(settingsJson),
    tracks: JSON.parse(tracksJson),
    outputDir,
    overwrite: Boolean(overwrite),
  }));
}

// Pick the output folder for a batch run before encoding. Returns the chosen
// absolute path, or "" if the user cancelled.
export async function pickOutputDir() {
  const dialog = tauriDialog();
  if (!dialog || !dialog.open) return "";
  const dir = await dialog.open({ directory: true, multiple: false });
  return dir || "";
}

// Subscribe to per-file encode progress. `callback(jobId, fraction)` fires as
// FFmpeg reports progress (fraction in 0..1).
export function listenEncodeProgress(callback) {
  const event = window.__TAURI__ && window.__TAURI__.event;
  if (!event || !event.listen) return;
  event.listen("webcoder-encode-progress", (e) => {
    const payload = e && e.payload;
    if (payload && typeof payload.job_id === "string") {
      callback(payload.job_id, payload.fraction || 0);
    }
  });
}
