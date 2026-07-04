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
//!
//! The implementation is split by concern:
//! - [`auth`] — the Basic-auth / API-key request gate.
//! - [`handlers`] — the HTTP API endpoints and their request/response shapes.
//! - [`ffmpeg`] — wrappers around the system `ffmpeg`/`ffprobe` binaries.
//! - [`error`] — the shared JSON [`error::ApiError`] type.

mod auth;
mod error;
mod ffmpeg;
mod handlers;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};

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

    let encoders = ffmpeg::probe_encoders(&config.ffmpeg).await?;
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

    let api = handlers::router().layer(DefaultBodyLimit::max(config.max_upload_bytes));

    let static_service =
        tower_http::services::ServeDir::new(&config.dist).append_index_html_on_directories(true);

    let app = Router::new()
        .merge(api)
        .fallback_service(static_service)
        .layer(middleware::from_fn_with_state(state.clone(), auth::auth_layer))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
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
