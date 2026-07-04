//! The HTTP API endpoints: encoder listing, job creation (upload or native
//! path), encoding, single-output download, and multi-job zip streaming.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use webcoder_frontend::core::{self, ConvertSettings, Track};

use super::error::ApiError;
use super::ffmpeg::{find_output, probe_input, tail};
use super::{EncoderInfo, JobEntry, Shared};

/// The API sub-router, mounted under the auth layer by [`super::serve`].
pub(crate) fn router() -> Router<Shared> {
    Router::new()
        .route("/api/encoders", get(get_encoders))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/from-path", post(create_job_from_path))
        .route("/api/jobs/:id/encode", post(encode_job))
        .route("/api/jobs/:id/output", get(get_output))
        .route("/api/zip", get(zip_outputs))
}

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

    core::validate_job(&req.settings, &req.tracks, stream_count, &state.encoder_names)
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

async fn get_output(State(state): State<Shared>, Path(id): Path<String>) -> Result<Response, ApiError> {
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
