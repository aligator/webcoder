//! Native HTTP backend: serves the built frontend and runs the real system
//! FFmpeg on uploaded media. Unlike the in-browser WASM core this has full
//! codec support (AV1 decode via the host's dav1d, etc.).
//!
//! Security model:
//! - No shell is ever invoked. FFmpeg is spawned with an explicit argv vector
//!   (`tokio::process::Command`), so there is no word-splitting / injection.
//! - The whole command line is rebuilt server-side from *structured, validated*
//!   settings (`core::validate_job` + `core::build_args`). There is no
//!   free-form custom-argument field — the client cannot inject flags.
//! - Every job runs in its own throwaway directory. Input and output paths are
//!   chosen by the server; the client filename only contributes a sanitized
//!   extension/stem. No path traversal or absolute-path escape is possible.
//! - `-nostdin` and an input `-protocol_whitelist file,pipe` block FFmpeg
//!   protocol tricks (http/concat/subfile/…).
//! - Uploads are size-capped, jobs are concurrency-limited and wall-clock
//!   timed out, and working dirs are swept on a TTL.
//! - Optional HTTP Basic auth (env `WEBCODER_AUTH=user:pass`) gates everything.
//!   TLS is expected to be terminated by a reverse proxy.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, Semaphore};

use webcoder_frontend::core::{self, ConvertSettings, StreamKind, Track, TrackOutput};

#[derive(Clone)]
struct Config {
    dist: PathBuf,
    workdir: PathBuf,
    max_upload_bytes: usize,
    job_timeout: Duration,
    job_ttl: Duration,
    ffmpeg: String,
    ffprobe: String,
    auth: Option<(String, String)>,
    api_key: Option<String>,
}

impl Config {
    fn from_env() -> Self {
        let mb = |key: &str, default: usize| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let secs = |key: &str, default: u64| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(default)
        };
        let auth = std::env::var("WEBCODER_AUTH").ok().and_then(|raw| {
            raw.split_once(':')
                .map(|(u, p)| (u.to_owned(), p.to_owned()))
        });
        Self {
            dist: std::env::var("WEBCODER_DIST")
                .unwrap_or_else(|_| "dist".into())
                .into(),
            workdir: std::env::var("WEBCODER_WORKDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir().join("webcoder")),
            max_upload_bytes: mb("WEBCODER_MAX_UPLOAD_MB", 4096) * 1024 * 1024,
            job_timeout: Duration::from_secs(secs("WEBCODER_JOB_TIMEOUT_SECS", 3600)),
            job_ttl: Duration::from_secs(secs("WEBCODER_JOB_TTL_SECS", 3600)),
            ffmpeg: std::env::var("WEBCODER_FFMPEG").unwrap_or_else(|_| "ffmpeg".into()),
            ffprobe: std::env::var("WEBCODER_FFPROBE").unwrap_or_else(|_| "ffprobe".into()),
            auth,
            api_key: None,
        }
    }
}

struct JobEntry {
    dir: PathBuf,
    input: PathBuf,
    stream_count: usize,
    created: Instant,
}

struct AppState {
    config: Config,
    encoders: Vec<EncoderInfo>,
    encoder_names: HashSet<String>,
    jobs: Mutex<HashMap<String, JobEntry>>,
    permits: Semaphore,
}

type Shared = Arc<AppState>;

#[derive(Serialize, Clone)]
struct EncoderInfo {
    name: String,
    kind: String,
    description: String,
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();
    let addr = std::env::var("WEBCODER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("webcoder: listening on http://{addr}");
    serve(config, listener).await
}

pub async fn run_with_listener(
    listener: tokio::net::TcpListener,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();
    let addr = listener.local_addr()?;
    eprintln!("webcoder: listening on http://{addr}");
    serve(config, listener).await
}

pub async fn run_with_paths(
    listener: tokio::net::TcpListener,
    dist: PathBuf,
    workdir: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    run_with_paths_and_key(listener, dist, workdir, None).await
}

pub async fn run_with_paths_and_key(
    listener: tokio::net::TcpListener,
    dist: PathBuf,
    workdir: PathBuf,
    api_key: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::from_env();
    config.dist = dist;
    config.workdir = workdir;
    config.api_key = api_key;
    let addr = listener.local_addr()?;
    eprintln!("webcoder: listening on http://{addr}");
    serve(config, listener).await
}

async fn serve(
    config: Config,
    listener: tokio::net::TcpListener,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::create_dir_all(&config.workdir).await?;

    let encoders = probe_encoders(&config.ffmpeg).await?;
    let encoder_names = encoders.iter().map(|e| e.name.clone()).collect();
    eprintln!("webcoder: detected {} encoders", encoders.len());

    let max_concurrent = std::env::var("WEBCODER_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .max(1);

    let state: Shared = Arc::new(AppState {
        encoders,
        encoder_names,
        jobs: Mutex::new(HashMap::new()),
        permits: Semaphore::new(max_concurrent),
        config: config.clone(),
    });

    spawn_cleanup(state.clone());

    let api = Router::new()
        .route("/api/encoders", get(get_encoders))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/from-path", post(create_job_from_path))
        .route("/api/jobs/:id/encode", post(encode_job))
        .route("/api/jobs/:id/output", get(get_output))
        .route("/api/zip", get(zip_outputs))
        .layer(DefaultBodyLimit::max(config.max_upload_bytes));

    let static_service =
        tower_http::services::ServeDir::new(&config.dist).append_index_html_on_directories(true);

    let app = Router::new()
        .merge(api)
        .fallback_service(static_service)
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
}

// --- Auth ---------------------------------------------------------------

async fn auth_layer(
    State(state): State<Shared>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or_default().to_owned();

    let Some((user, pass)) = &state.config.auth else {
        if api_key_allowed(&state.config.api_key, &headers, &query, &path) {
            return next.run(request).await;
        }
        return unauthorized("API key required.");
    };

    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        if let Some((u, p)) = value.split_once(':') {
            if constant_eq(u, user) && constant_eq(p, pass) {
                if api_key_allowed(&state.config.api_key, &headers, &query, &path) {
                    return next.run(request).await;
                }
                return unauthorized("API key required.");
            }
        }
    }

    unauthorized("Authentication required.")
}

fn unauthorized(message: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"webcoder\"")],
        message,
    )
        .into_response()
}

fn api_key_allowed(
    expected: &Option<String>,
    headers: &HeaderMap,
    query: &str,
    path: &str,
) -> bool {
    if !path.starts_with("/api/") && path != "/api" {
        return true;
    }
    let Some(expected) = expected else {
        return true;
    };
    let header_key = headers.get("x-webcoder-key").and_then(|v| v.to_str().ok());
    let query_key = query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "webcoder_key").then_some(value)
    });
    header_key
        .or(query_key)
        .is_some_and(|value| constant_eq(value, expected))
}

/// Compare in time independent of where the first mismatch is, so a matching
/// prefix can't be discovered via response timing.
fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// --- Handlers -----------------------------------------------------------

async fn get_encoders(State(state): State<Shared>) -> Json<Vec<EncoderInfo>> {
    Json(state.encoders.clone())
}

#[derive(Serialize)]
struct CreateJobResponse {
    job_id: String,
    stream_count: usize,
    tracks: Vec<Track>,
    format: Option<Value>,
    file_name: Option<String>,
    size_bytes: Option<u64>,
}

async fn create_job(
    State(state): State<Shared>,
    mut multipart: Multipart,
) -> Result<Json<CreateJobResponse>, ApiError> {
    let job_id = uuid::Uuid::new_v4().to_string();
    let dir = state.config.workdir.join(&job_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::internal(format!("create job dir: {e}")))?;

    let mut input_path: Option<PathBuf> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("multipart: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let ext = field
            .file_name()
            .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_owned()))
            .map(|e| core::safe_stem(&e))
            .filter(|e| !e.is_empty() && e.len() <= 8)
            .unwrap_or_else(|| "bin".into());
        let path = dir.join(format!("input.{ext}"));
        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError::bad_request(format!("upload read: {e}")))?;
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| ApiError::internal(format!("write input: {e}")))?;
        input_path = Some(path);
    }

    let Some(input_path) = input_path else {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(ApiError::bad_request("No file field in upload."));
    };

    let (tracks, stream_count, format) = probe_input(&state.config.ffprobe, &input_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("probe failed: {e}")))?;

    state.jobs.lock().await.insert(
        job_id.clone(),
        JobEntry {
            dir,
            input: input_path,
            stream_count,
            created: Instant::now(),
        },
    );

    Ok(Json(CreateJobResponse {
        job_id,
        stream_count,
        tracks,
        format,
        file_name: None,
        size_bytes: None,
    }))
}

#[derive(Deserialize)]
struct CreateJobFromPathRequest {
    path: PathBuf,
}

async fn create_job_from_path(
    State(state): State<Shared>,
    Json(req): Json<CreateJobFromPathRequest>,
) -> Result<Json<CreateJobResponse>, ApiError> {
    if state.config.api_key.is_none() {
        return Err(ApiError::forbidden(
            "Path-based jobs are only available in desktop mode.",
        ));
    }
    if !req.path.is_file() {
        return Err(ApiError::bad_request("Selected path is not a file."));
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let dir = state.config.workdir.join(&job_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::internal(format!("create job dir: {e}")))?;

    let metadata = tokio::fs::metadata(&req.path)
        .await
        .map_err(|e| ApiError::bad_request(format!("read file metadata: {e}")))?;
    let (tracks, stream_count, format) = probe_input(&state.config.ffprobe, &req.path)
        .await
        .map_err(|e| ApiError::bad_request(format!("probe failed: {e}")))?;
    let file_name = req
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());

    state.jobs.lock().await.insert(
        job_id.clone(),
        JobEntry {
            dir,
            input: req.path,
            stream_count,
            created: Instant::now(),
        },
    );

    Ok(Json(CreateJobResponse {
        job_id,
        stream_count,
        tracks,
        format,
        file_name,
        size_bytes: Some(metadata.len()),
    }))
}

#[derive(Deserialize)]
struct EncodeRequest {
    settings: ConvertSettings,
    tracks: Vec<Track>,
}

#[derive(Serialize)]
struct EncodeResponse {
    ok: bool,
    log: String,
    output_name: String,
    download_url: String,
}

async fn encode_job(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<EncodeRequest>,
) -> Result<Json<EncodeResponse>, ApiError> {
    let (dir, input, stream_count) = {
        let jobs = state.jobs.lock().await;
        let entry = jobs
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Unknown or expired job."))?;
        (entry.dir.clone(), entry.input.clone(), entry.stream_count)
    };

    core::validate_job(
        &req.settings,
        &req.tracks,
        stream_count,
        &state.encoder_names,
    )
    .map_err(ApiError::unprocessable)?;

    let output_name = format!(
        "{}.{}",
        core::safe_stem(&req.settings.output_name),
        req.settings.container
    );
    // Output lives in a dedicated `out/` subdir named with the friendly output
    // name, so downloads and zip entries carry that name (and never collide
    // with the `input.*` file).
    let out_dir = dir.join("out");
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| ApiError::internal(format!("create out dir: {e}")))?;
    let output = out_dir.join(&output_name);

    let input_str = input.to_string_lossy().into_owned();
    let output_str = output.to_string_lossy().into_owned();

    // Sandbox-hardening options prepended before the input; then the fully
    // structured, validated argument list from core (no shell, no free args).
    let mut args = vec![
        "-nostdin".to_owned(),
        "-protocol_whitelist".to_owned(),
        "file,pipe".to_owned(),
    ];
    args.extend(core::build_args(
        &req.settings,
        &req.tracks,
        &input_str,
        &output_str,
        false,
    ));

    let _permit = state
        .permits
        .acquire()
        .await
        .map_err(|_| ApiError::internal("scheduler closed"))?;

    let run = tokio::time::timeout(
        state.config.job_timeout,
        tokio::process::Command::new(&state.config.ffmpeg)
            .args(&args)
            .current_dir(&dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output_result = match run {
        Err(_) => return Err(ApiError::unprocessable("Encode timed out.")),
        Ok(Err(e)) => return Err(ApiError::internal(format!("spawn ffmpeg: {e}"))),
        Ok(Ok(out)) => out,
    };

    let log = String::from_utf8_lossy(&output_result.stderr).into_owned();
    if !output_result.status.success() {
        return Ok(Json(EncodeResponse {
            ok: false,
            log: format!(
                "FFmpeg exited with {}.\n\n{}",
                output_result.status,
                tail(&log, 8000)
            ),
            output_name,
            download_url: String::new(),
        }));
    }

    Ok(Json(EncodeResponse {
        ok: true,
        log: tail(&log, 8000),
        output_name,
        download_url: format!("/api/jobs/{id}/output"),
    }))
}

async fn get_output(
    State(state): State<Shared>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let dir = {
        let jobs = state.jobs.lock().await;
        jobs.get(&id)
            .map(|e| e.dir.clone())
            .ok_or_else(|| ApiError::not_found("Unknown or expired job."))?
    };
    let output = find_output(&dir)
        .await
        .ok_or_else(|| ApiError::not_found("No output produced yet."))?;
    let file_name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let data = tokio::fs::read(&output)
        .await
        .map_err(|e| ApiError::internal(format!("read output: {e}")))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
        ],
        Body::from(data),
    )
        .into_response())
}

/// Stream a zip of several jobs' outputs directly to the client. Jobs are
/// passed as `?jobs=id1,id2,...`. The archive is assembled to a temp file
/// (stored, no recompression — the media is already compressed) then streamed
/// with a `ReaderStream`, so a large batch never has to sit in memory.
async fn zip_outputs(
    State(state): State<Shared>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let ids: Vec<String> = query
        .get("jobs")
        .map(|s| {
            s.split(',')
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(ApiError::bad_request("No jobs specified."));
    }

    // Resolve each job's directory under the lock, then find outputs off-lock.
    let dirs: Vec<PathBuf> = {
        let jobs = state.jobs.lock().await;
        ids.iter()
            .filter_map(|id| jobs.get(id).map(|e| e.dir.clone()))
            .collect()
    };

    let mut entries: Vec<(PathBuf, String)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for dir in &dirs {
        if let Some(path) = find_output(dir).await {
            let base = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "output".into());
            // Disambiguate identical output names within the archive.
            let mut name = base.clone();
            let mut n = 2;
            while !used.insert(name.clone()) {
                name = match base.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                    None => format!("{base} ({n})"),
                };
                n += 1;
            }
            entries.push((path, name));
        }
    }
    if entries.is_empty() {
        return Err(ApiError::not_found("No finished outputs to zip."));
    }

    // Stream the archive live: a background task writes zip entries into one end
    // of an in-memory pipe while the HTTP response reads from the other. Uses
    // async_zip's streaming writer (data descriptors), so nothing is buffered to
    // disk or held whole in memory — just one copy buffer at a time.
    let (writer, reader) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        if let Err(error) = write_zip_stream(writer, entries).await {
            eprintln!("zip stream error: {error}");
        }
    });
    let stream = tokio_util::io::ReaderStream::new(reader);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"webcoder-batch.zip\"".to_owned(),
            ),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn write_zip_stream<W>(
    sink: W,
    entries: Vec<(PathBuf, String)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use async_zip::base::write::ZipFileWriter;
    use async_zip::{Compression, ZipEntryBuilder};

    use tokio_util::compat::TokioAsyncReadCompatExt;

    let mut zip = ZipFileWriter::with_tokio(sink);
    for (path, name) in entries {
        let builder = ZipEntryBuilder::new(name.into(), Compression::Stored);
        let mut entry = zip.write_entry_stream(builder).await?;
        // async_zip's entry writer is a futures AsyncWrite; adapt the tokio File
        // to a futures AsyncRead and copy with futures-lite.
        let mut source = tokio::fs::File::open(&path).await?.compat();
        futures_lite::io::copy(&mut source, &mut entry).await?;
        entry.close().await?;
    }
    zip.close().await?;
    Ok(())
}

// --- FFmpeg helpers -----------------------------------------------------

async fn probe_encoders(ffmpeg: &str) -> Result<Vec<EncoderInfo>, Box<dyn std::error::Error>> {
    let out = tokio::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse_encoders(&text))
}

fn parse_encoders(text: &str) -> Vec<EncoderInfo> {
    // Lines look like: " V....D libx264   libx264 H.264 / AVC ..."
    let mut seen = HashSet::new();
    let mut list = Vec::new();
    for line in text.lines() {
        let bytes = line.as_bytes();
        // Need leading whitespace, 6 flag chars, space, then name.
        let trimmed = line.trim_start();
        if trimmed.len() < 8 {
            continue;
        }
        let flags = &trimmed[..6];
        let kind = match flags.as_bytes()[0] {
            b'V' => "Video",
            b'A' => "Audio",
            b'S' => "Subtitle",
            _ => continue,
        };
        if !flags[1..]
            .bytes()
            .all(|c| c == b'.' || c.is_ascii_alphabetic())
        {
            continue;
        }
        let rest = trimmed[6..].trim_start();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else { continue };
        if name.is_empty() || !name.bytes().next().unwrap_or(b' ').is_ascii_alphanumeric() {
            continue;
        }
        let description = parts.next().unwrap_or("").trim().to_owned();
        let _ = bytes;
        if seen.insert(name.to_owned()) {
            list.push(EncoderInfo {
                name: name.to_owned(),
                kind: kind.to_owned(),
                description,
            });
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

async fn probe_input(
    ffprobe: &str,
    input: &PathBuf,
) -> Result<(Vec<Track>, usize, Option<Value>), Box<dyn std::error::Error>> {
    let out = tokio::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(input)
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    let json: Value = serde_json::from_slice(&out.stdout)?;
    let streams = json.get("streams").and_then(|v| v.as_array()).cloned();
    let format = json.get("format").cloned();

    let mut tracks = Vec::new();
    if let Some(streams) = &streams {
        for (fallback, stream) in streams.iter().enumerate() {
            let index = stream
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(fallback);
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let kind = match codec_type {
                "video" => StreamKind::Video,
                "audio" => StreamKind::Audio,
                "subtitle" => StreamKind::Subtitle,
                _ => StreamKind::Attachment,
            };
            let codec = stream
                .get("codec_long_name")
                .and_then(|v| v.as_str())
                .or_else(|| stream.get("codec_name").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_owned();
            let tags = stream.get("tags");
            let language = tags
                .and_then(|t| t.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("und")
                .to_owned();
            let title = tags
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
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
    let stream_count = streams.map(|s| s.len()).unwrap_or(0);
    Ok((tracks, stream_count, format))
}

async fn find_output(dir: &PathBuf) -> Option<PathBuf> {
    // The encode writes its single result into the job's `out/` subdir under the
    // friendly output name; return that file.
    let mut rd = tokio::fs::read_dir(dir.join("out")).await.ok()?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if entry.path().is_file() {
            return Some(entry.path());
        }
    }
    None
}

fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let start = text.len() - max;
    format!("…\n{}", &text[start..])
}

fn spawn_cleanup(state: Shared) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let ttl = state.config.job_ttl;
            let mut expired = Vec::new();
            {
                let mut jobs = state.jobs.lock().await;
                jobs.retain(|id, entry| {
                    if entry.created.elapsed() > ttl {
                        expired.push((id.clone(), entry.dir.clone()));
                        false
                    } else {
                        true
                    }
                });
            }
            for (_, dir) in expired {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
        }
    });
}

// --- Errors -------------------------------------------------------------

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    fn bad_request(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, m)
    }
    fn unprocessable(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, m)
    }
    fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, m)
    }
    fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, m)
    }
    fn internal(m: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, m)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}
