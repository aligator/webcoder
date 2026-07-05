use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::{Mutex, Semaphore};
use webcoder_frontend::core::{self, ConvertSettings, StreamKind, Track, TrackOutput};

#[derive(Clone)]
pub struct NativeBackend {
    inner: Arc<Inner>,
}

struct Inner {
    ffmpeg: String,
    ffprobe: String,
    encoders: Vec<EncoderInfo>,
    encoder_names: HashSet<String>,
    jobs: Mutex<HashMap<String, JobEntry>>,
    permits: Semaphore,
    job_timeout: Duration,
}

struct JobEntry {
    input: PathBuf,
    stream_count: usize,
    duration: Option<f64>,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    job_id: String,
    fraction: f64,
}

#[derive(Serialize, Clone)]
pub struct EncoderInfo {
    name: String,
    kind: String,
    description: String,
}

#[derive(Serialize)]
pub struct ProbeResponse {
    job_id: String,
    stream_count: usize,
    tracks: Vec<Track>,
    format: Option<Value>,
    file_name: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct EncodeResponse {
    ok: bool,
    log: String,
    output_name: String,
    download_url: String,
    output_path: Option<String>,
}

impl NativeBackend {
    pub async fn new() -> Result<Self, String> {
        let ffmpeg = std::env::var("WEBCODER_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
        let ffprobe = std::env::var("WEBCODER_FFPROBE").unwrap_or_else(|_| "ffprobe".into());
        let encoders = probe_encoders(&ffmpeg).await?;
        let encoder_names = encoders.iter().map(|encoder| encoder.name.clone()).collect();
        eprintln!("webcoder: detected {} encoders", encoders.len());
        Ok(Self {
            inner: Arc::new(Inner {
                ffmpeg,
                ffprobe,
                encoders,
                encoder_names,
                jobs: Mutex::new(HashMap::new()),
                permits: Semaphore::new(2),
                job_timeout: Duration::from_secs(3600),
            }),
        })
    }
}

#[tauri::command]
pub async fn get_encoders_native(state: State<'_, NativeBackend>) -> Result<Vec<EncoderInfo>, String> {
    Ok(state.inner.encoders.clone())
}

#[tauri::command]
pub async fn probe_native_path(
    state: State<'_, NativeBackend>,
    path: PathBuf,
) -> Result<ProbeResponse, String> {
    if !path.is_file() {
        return Err("Selected path is not a file.".into());
    }
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| format!("read file metadata: {error}"))?;
    let (tracks, stream_count, format) = probe_input(&state.inner.ffprobe, &path).await?;
    let job_id = uuid::Uuid::new_v4().to_string();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    // Desktop mode encodes straight into the user-chosen output folder, so no
    // temp/job directory is needed — only the total duration for progress.
    let duration = format
        .as_ref()
        .and_then(|value| value.get("duration"))
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<f64>().ok());

    state.inner.jobs.lock().await.insert(
        job_id.clone(),
        JobEntry {
            input: path,
            stream_count,
            duration,
        },
    );

    Ok(ProbeResponse {
        job_id,
        stream_count,
        tracks,
        format,
        file_name,
        size_bytes: Some(metadata.len()),
    })
}

#[tauri::command]
pub async fn encode_native(
    app: AppHandle,
    state: State<'_, NativeBackend>,
    job_id: String,
    settings: ConvertSettings,
    tracks: Vec<Track>,
    output_dir: PathBuf,
    overwrite: bool,
) -> Result<EncodeResponse, String> {
    let (input, stream_count, duration) = {
        let jobs = state.inner.jobs.lock().await;
        let entry = jobs
            .get(&job_id)
            .ok_or_else(|| "Unknown or expired job.".to_owned())?;
        (entry.input.clone(), entry.stream_count, entry.duration)
    };

    if !output_dir.is_dir() {
        return Err("Output folder does not exist.".into());
    }

    core::validate_job(&settings, &tracks, stream_count, &state.inner.encoder_names)?;
    let output_name = format!("{}.{}", core::safe_stem(&settings.output_name), settings.container);
    // Write straight into the chosen output folder — no temp files on desktop.
    let output = output_dir.join(&output_name);
    if !overwrite && output.exists() {
        return Ok(EncodeResponse {
            ok: false,
            log: format!(
                "Output file already exists: {output_name}\nEnable \"Overwrite existing files\" to replace it."
            ),
            output_name,
            download_url: String::new(),
            output_path: None,
        });
    }
    let input_str = input.to_string_lossy().into_owned();
    let output_str = output.to_string_lossy().into_owned();
    // `-progress pipe:1` streams key=value progress on stdout so we can emit a
    // per-file fraction to the frontend; `-nostats` silences the stderr status
    // spam while keeping real warnings/errors for the log.
    let mut args = vec![
        "-nostdin".to_owned(),
        "-progress".to_owned(),
        "pipe:1".to_owned(),
        "-nostats".to_owned(),
        "-protocol_whitelist".to_owned(),
        "file,pipe".to_owned(),
    ];
    args.extend(core::build_args(
        &settings,
        &tracks,
        &input_str,
        &output_str,
        false,
    ));

    let _permit = state
        .inner
        .permits
        .acquire()
        .await
        .map_err(|_| "scheduler closed".to_owned())?;

    let mut child = tokio::process::Command::new(&state.inner.ffmpeg)
        .args(&args)
        .current_dir(&output_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("spawn ffmpeg: {error}"))?;

    let stdout = child.stdout.take().ok_or("no ffmpeg stdout")?;
    let stderr = child.stderr.take().ok_or("no ffmpeg stderr")?;
    let progress = tokio::spawn(pump_progress(stdout, app.clone(), job_id.clone(), duration));
    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let mut stderr = stderr;
        let _ = stderr.read_to_end(&mut buffer).await;
        buffer
    });

    let status = match tokio::time::timeout(state.inner.job_timeout, child.wait()).await {
        Err(_) => {
            let _ = child.kill().await;
            progress.abort();
            return Err("Encode timed out.".into());
        }
        Ok(Err(error)) => return Err(format!("run ffmpeg: {error}")),
        Ok(Ok(status)) => status,
    };
    let _ = progress.await;
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    let log = String::from_utf8_lossy(&stderr_bytes).into_owned();

    if !status.success() {
        emit_progress(&app, &job_id, 0.0);
        return Ok(EncodeResponse {
            ok: false,
            log: format!("FFmpeg exited with {}.\n\n{}", status, tail(&log, 8000)),
            output_name,
            download_url: String::new(),
            output_path: None,
        });
    }

    emit_progress(&app, &job_id, 1.0);
    Ok(EncodeResponse {
        ok: true,
        log: tail(&log, 8000),
        output_name,
        download_url: String::new(),
        output_path: Some(output.to_string_lossy().into_owned()),
    })
}

/// Read FFmpeg's `-progress` stream and emit a per-file completion fraction to
/// the frontend on every progress block.
async fn pump_progress(
    stdout: tokio::process::ChildStdout,
    app: AppHandle,
    job_id: String,
    duration: Option<f64>,
) {
    let mut lines = BufReader::new(stdout).lines();
    let mut out_time_us: f64 = 0.0;
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(value) = line.strip_prefix("out_time_us=") {
            out_time_us = value.trim().parse().unwrap_or(out_time_us);
        } else if line.starts_with("progress=") {
            let fraction = match duration {
                Some(total) if total > 0.0 => (out_time_us / 1_000_000.0 / total).clamp(0.0, 1.0),
                _ => 0.0,
            };
            emit_progress(&app, &job_id, fraction);
        }
    }
}

fn emit_progress(app: &AppHandle, job_id: &str, fraction: f64) {
    let _ = app.emit(
        "webcoder-encode-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            fraction,
        },
    );
}

async fn probe_encoders(ffmpeg: &str) -> Result<Vec<EncoderInfo>, String> {
    let out = tokio::process::Command::new(ffmpeg)
        .arg("-encoders")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| format!("spawn ffmpeg: {error}"))?;
    if !out.status.success() {
        return Err(format!("ffmpeg -encoders: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut list = Vec::new();
    let mut seen = HashSet::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.len() < 8 || !line.starts_with(|c: char| matches!(c, 'V' | 'A' | 'S')) {
            continue;
        }
        let mut parts = line.split_whitespace();
        let flags = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let kind = match flags.chars().next().unwrap_or(' ') {
            'V' => "Video",
            'A' => "Audio",
            'S' => "Subtitle",
            _ => continue,
        };
        let description = parts.collect::<Vec<_>>().join(" ");
        if seen.insert(name.to_owned()) {
            list.push(EncoderInfo {
                name: name.to_owned(),
                kind: kind.to_owned(),
                description,
            });
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

async fn probe_input(ffprobe: &str, input: &PathBuf) -> Result<(Vec<Track>, usize, Option<Value>), String> {
    let out = tokio::process::Command::new(ffprobe)
        .args(["-v", "error", "-show_format", "-show_streams", "-of", "json"])
        .arg(input)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| format!("spawn ffprobe: {error}"))?;
    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|error| format!("parse ffprobe: {error}"))?;
    let streams = json.get("streams").and_then(|value| value.as_array()).cloned();
    let format = json.get("format").cloned();
    let mut tracks = Vec::new();
    if let Some(streams) = &streams {
        for (fallback, stream) in streams.iter().enumerate() {
            let index = stream
                .get("index")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
                .unwrap_or(fallback);
            let kind = match stream.get("codec_type").and_then(|value| value.as_str()).unwrap_or("") {
                "video" => StreamKind::Video,
                "audio" => StreamKind::Audio,
                "subtitle" => StreamKind::Subtitle,
                _ => StreamKind::Attachment,
            };
            let codec = stream
                .get("codec_long_name")
                .and_then(|value| value.as_str())
                .or_else(|| stream.get("codec_name").and_then(|value| value.as_str()))
                .unwrap_or("unknown")
                .to_owned();
            let tags = stream.get("tags");
            let language = tags
                .and_then(|tags| tags.get("language"))
                .and_then(|value| value.as_str())
                .unwrap_or("und")
                .to_owned();
            let title = tags
                .and_then(|tags| tags.get("title"))
                .and_then(|value| value.as_str())
                .unwrap_or(kind.label())
                .to_owned();
            tracks.push(Track {
                id: fallback + 1,
                source_index: index,
                enabled: true,
                kind,
                codec,
                language,
                title,
                choice: TrackOutput::Copy,
            });
        }
    }
    let stream_count = streams.map(|streams| streams.len()).unwrap_or(0);
    Ok((tracks, stream_count, format))
}

fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let start = text.len() - max;
    format!("…\n{}", &text[start..])
}
