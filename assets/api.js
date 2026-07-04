// Thin fetch bridge between the Yew/WASM frontend and the Rust backend.
// Every function returns a JSON *string* so the Rust side can deserialize it
// straight into typed structs with serde. Requests are same-origin, so the
// browser attaches any HTTP Basic credentials automatically. The Tauri shell
// adds a random per-process API key in the URL; browsers use it for local API
// calls, including download links that cannot attach custom headers.

const API_KEY_PARAM = "webcoder_key";

function tauriDialog() {
  return window.__TAURI__ && window.__TAURI__.dialog;
}

function tauriInvoke() {
  return window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
}

function isTauri() {
  return Boolean(tauriInvoke());
}

export function nativeApp() {
  return isTauri();
}

export function nativePickerAvailable() {
  return Boolean(tauriDialog() && tauriDialog().open);
}

// Subscribe to the desktop shell's OS drag-drop. `callback` receives an array
// of absolute file paths. No-op outside Tauri (browsers use HTML DnD).
//
// The Rust side catches the window DragDrop event and re-emits the paths as the
// plain app event below, which a normal `event.listen` reliably receives.
export function listenNativeDrop(callback) {
  console.log("[dnd] listenNativeDrop called, isTauri=", isTauri());
  if (!isTauri()) return;
  const event = window.__TAURI__ && window.__TAURI__.event;
  console.log("[dnd] __TAURI__.event present=", Boolean(event && event.listen));
  if (!event || !event.listen) return;
  event.listen("webcoder-files-dropped", (e) => {
    console.log("[dnd] received event", e && e.payload);
    const paths = e && e.payload;
    if (Array.isArray(paths) && paths.length) callback(paths);
  });
  console.log("[dnd] listener registered for webcoder-files-dropped");
}

function apiKey() {
  const params = new URLSearchParams(window.location.search);
  const key = params.get(API_KEY_PARAM) || sessionStorage.getItem(API_KEY_PARAM);
  if (key) sessionStorage.setItem(API_KEY_PARAM, key);
  return key || "";
}

function withKey(url) {
  const key = apiKey();
  if (!key) return url;
  const separator = url.includes("?") ? "&" : "?";
  return `${url}${separator}${API_KEY_PARAM}=${encodeURIComponent(key)}`;
}

function fetchOptions(options = {}) {
  const key = apiKey();
  if (!key) return options;
  return {
    ...options,
    headers: {
      ...(options.headers || {}),
      "x-webcoder-key": key,
    },
  };
}

async function readError(response) {
  try {
    const data = await response.json();
    if (data && data.error) return data.error;
  } catch (_) {
    // fall through to status text
  }
  return `${response.status} ${response.statusText}`;
}

export async function getEncoders() {
  if (isTauri()) {
    return JSON.stringify(await tauriInvoke()("get_encoders_native"));
  }
  const response = await fetch("/api/encoders", fetchOptions());
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}

// Uploads the file and probes it. Returns { job_id, stream_count, tracks }.
export async function probeMedia(file) {
  if (isTauri()) throw new Error("Use native file picker in desktop mode.");
  const form = new FormData();
  form.append("file", file, file.name);
  const response = await fetch("/api/jobs", fetchOptions({ method: "POST", body: form }));
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}

export async function pickNativeFiles() {
  const dialog = tauriDialog();
  if (!dialog || !dialog.open) return "[]";
  const picked = await dialog.open({ multiple: true, directory: false });
  if (!picked) return "[]";
  return JSON.stringify(Array.isArray(picked) ? picked : [picked]);
}

export async function probeNativePath(path) {
  if (isTauri()) {
    return JSON.stringify(await tauriInvoke()("probe_native_path", { path }));
  }
  const response = await fetch("/api/jobs/from-path", fetchOptions({
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path }),
  }));
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}

// Runs the encode for a previously-probed job. settingsJson/tracksJson are
// JSON strings produced by serde on the Rust side; forwarded verbatim.
export async function runEncode(jobId, settingsJson, tracksJson) {
  if (isTauri()) {
    return JSON.stringify(await tauriInvoke()("encode_native", {
      jobId,
      settings: JSON.parse(settingsJson),
      tracks: JSON.parse(tracksJson),
    }));
  }
  const response = await fetch(`/api/jobs/${encodeURIComponent(jobId)}/encode`, fetchOptions({
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: `{"settings":${settingsJson},"tracks":${tracksJson}}`,
  }));
  if (!response.ok) throw new Error(await readError(response));
  const data = await response.json();
  if (data.download_url) data.download_url = withKey(data.download_url);
  return JSON.stringify(data);
}

export function withApiKey(url) {
  return withKey(url);
}

export async function saveOutput(outputPath, outputName) {
  if (!isTauri()) return false;
  const dialog = tauriDialog();
  if (!dialog || !dialog.save) throw new Error("Save dialog unavailable.");
  const destination = await dialog.save({ defaultPath: outputName || "output" });
  if (!destination) return false;
  await tauriInvoke()("save_output_native", { outputPath, destination });
  return true;
}

// Pick one destination folder, then copy every finished output into it.
// `itemsJson` is a JSON array of { output_path, output_name }.
export async function saveAllOutputs(itemsJson) {
  if (!isTauri()) return JSON.stringify({ saved: 0 });
  const dialog = tauriDialog();
  if (!dialog || !dialog.open) throw new Error("Folder dialog unavailable.");
  const dir = await dialog.open({ directory: true, multiple: false });
  if (!dir) return JSON.stringify({ saved: 0 });
  const items = JSON.parse(itemsJson);
  let saved = 0;
  for (const item of items) {
    const destination = `${dir}/${item.output_name}`;
    await tauriInvoke()("save_output_native", { outputPath: item.output_path, destination });
    saved += 1;
  }
  return JSON.stringify({ saved });
}
