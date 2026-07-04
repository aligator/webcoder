import { FFmpeg } from "/assets/vendor/ffmpeg/ffmpeg/index.js";

let ffmpeg;
let loaded = false;
let lastLog = "";
let logLines = [];
let capabilityCache = null;

async function ensureFfmpeg() {
  if (!ffmpeg) {
    ffmpeg = new FFmpeg();
    ffmpeg.on("log", ({ message }) => {
      lastLog = message;
      logLines.push(message);
    });
  }

  if (!loaded) {
    const base = new URL("/assets/vendor/ffmpeg/core/", self.location.origin);
    await ffmpeg.load({
      coreURL: new URL("ffmpeg-core.js", base).href,
      wasmURL: new URL("ffmpeg-core.wasm", base).href,
    });
    loaded = true;
  }
}

export async function probeMedia(file, inputName) {
  await ensureFfmpeg();
  logLines = [];

  await writeInput(file, inputName);

  try {
    const probeOutput = `${inputName}.ffprobe.json`;
    const exitCode = await ffmpeg.ffprobe([
      "-v",
      "error",
      "-show_format",
      "-show_streams",
      "-of",
      "json",
      inputName,
      "-o",
      probeOutput,
    ]);

    if (exitCode === 0) {
      const text = await ffmpeg.readFile(probeOutput, "utf8");
      const probe = JSON.parse(text);
      const streams = streamsFromFfprobe(probe);
      if (streams.length > 0) {
        return {
          log: logLines.join("\n"),
          streams,
          format: probe.format || null,
        };
      }
    }
  } catch (error) {
    logLines.push(`ffprobe JSON failed: ${error?.message || error}`);
  }

  await ffmpeg.exec(["-hide_banner", "-i", inputName]);

  const log = logLines.join("\n");
  return {
    log,
    streams: parseStreams(log),
  };
}

export async function getFfmpegCapabilities() {
  await ensureFfmpeg();
  if (capabilityCache) return capabilityCache;

  logLines = [];
  const exitCode = await ffmpeg.exec(["-hide_banner", "-encoders"]);
  const log = logLines.join("\n");
  capabilityCache = {
    exitCode,
    encoders: parseEncoders(log),
    log,
  };
  return capabilityCache;
}

export async function runFfmpeg(file, inputName, outputName, args) {
  await ensureFfmpeg();
  logLines = [];

  lastLog = "Writing input into FFmpeg virtual filesystem...";
  await ffmpeg.writeFile(inputName, new Uint8Array(await file.arrayBuffer()));

  lastLog = `ffmpeg ${args.join(" ")}`;
  const exitCode = await ffmpeg.exec(args);
  if (exitCode !== 0) {
    const log = logLines.join("\n");
    throw new Error(`FFmpeg exited with code ${exitCode}.\n\n${lastLog}\n\n${log}`);
  }

  const data = await ffmpeg.readFile(outputName);
  const blob = new Blob([data.buffer], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);

  return {
    name: outputName,
    url,
    log: `Browser encode complete: ${outputName}\n\n${logLines.join("\n")}`,
  };
}

async function writeInput(file, inputName) {
  await ffmpeg.writeFile(inputName, new Uint8Array(await file.arrayBuffer()));
}

function streamsFromFfprobe(probe) {
  return (probe.streams || []).map((stream, fallbackIndex) => {
    const tags = stream.tags || {};
    const kind = kindFromFfprobe(stream.codec_type);
    const details = [
      stream.width && stream.height ? `${stream.width}x${stream.height}` : "",
      stream.avg_frame_rate ? rateLabel(stream.avg_frame_rate) : "",
      stream.channels ? `${stream.channels} channels` : "",
      stream.channel_layout || "",
      stream.bit_rate ? `${Math.round(Number(stream.bit_rate) / 1000)} kb/s` : "",
    ].filter(Boolean);

    return {
      index: Number.isFinite(Number(stream.index)) ? Number(stream.index) : fallbackIndex,
      kind,
      codec: stream.codec_long_name || stream.codec_name || kind,
      language: tags.language || "und",
      title: tags.title || kind,
      details: details.join(" · "),
    };
  });
}

function kindFromFfprobe(codecType) {
  switch (codecType) {
    case "video":
      return "Video";
    case "audio":
      return "Audio";
    case "subtitle":
      return "Subtitle";
    case "attachment":
      return "Attachment";
    default:
      return "Data";
  }
}

function rateLabel(value) {
  if (!value || value === "0/0") return "";
  if (!value.includes("/")) return `${value} fps`;

  const [numerator, denominator] = value.split("/").map(Number);
  if (!numerator || !denominator) return "";
  const rate = numerator / denominator;
  return `${Number.isInteger(rate) ? rate : rate.toFixed(3)} fps`;
}

function parseEncoders(log) {
  const encoders = new Set();
  for (const line of log.split(/\r?\n/)) {
    const match = line.match(/^\s*[VAS]\S*\s+([A-Za-z0-9_.-]+)/);
    if (match) encoders.add(match[1]);
  }
  return [...encoders].sort((left, right) => left.localeCompare(right));
}

function parseStreams(log) {
  const streams = [];
  for (const line of log.split(/\r?\n/)) {
    const match = line.match(/Stream #0:(\d+)(?:\(([^)]+)\))?(?:\[[^\]]+\])?: (Video|Audio|Subtitle|Data|Attachment): ([^,\n]+)/);
    if (!match) continue;

    const [, index, lang, kind, codec] = match;
    const dims = line.match(/,\s*(\d{2,5})x(\d{2,5})[\s,]/);
    const fps = line.match(/,\s*([0-9.]+)\s*fps/);
    const channels = line.match(/,\s*(mono|stereo|5\.1|7\.1|[0-9]+ channels)[,\s]/i);
    const title = line.match(/title\s*:\s*(.+)$/i);

    streams.push({
      index: Number(index),
      kind,
      codec: codec.trim(),
      language: lang || "und",
      title: title ? title[1].trim() : kind,
      details: [dims ? `${dims[1]}x${dims[2]}` : "", fps ? `${fps[1]} fps` : "", channels ? channels[1] : ""]
        .filter(Boolean)
        .join(" · "),
    });
  }
  return streams;
}
