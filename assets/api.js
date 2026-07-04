// Thin fetch bridge between the Yew/WASM frontend and the Rust backend.
// Every function returns a JSON *string* so the Rust side can deserialize it
// straight into typed structs with serde. Requests are same-origin, so the
// browser attaches any HTTP Basic credentials automatically.

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
  const response = await fetch("/api/encoders");
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}

// Uploads the file and probes it. Returns { job_id, stream_count, tracks }.
export async function probeMedia(file) {
  const form = new FormData();
  form.append("file", file, file.name);
  const response = await fetch("/api/jobs", { method: "POST", body: form });
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}

// Runs the encode for a previously-probed job. settingsJson/tracksJson are
// JSON strings produced by serde on the Rust side; forwarded verbatim.
export async function runEncode(jobId, settingsJson, tracksJson) {
  const response = await fetch(`/api/jobs/${encodeURIComponent(jobId)}/encode`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: `{"settings":${settingsJson},"tracks":${tracksJson}}`,
  });
  if (!response.ok) throw new Error(await readError(response));
  return JSON.stringify(await response.json());
}
