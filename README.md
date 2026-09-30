# viz-rs

Reverse media search over Telegram channels: upload a photo or video (or type text) and get
the earliest post of that media in an origin channel and in a backup channel.

| Crate | Binary | Purpose |
|---|---|---|
| `crates/core` (`viz-core`) | – | CLIP ONNX embeddings, dhash + BK-tree, ffmpeg scene frames, S3 thumbnails, Qdrant, Postgres (migrations in `migrations/`) |
| `crates/ingest` (`viz-ingest`) | `ingest` | Telegram (MTProto via grammers) watcher and backfill |
| `crates/api` (`viz-api`) | `api` | axum search API used by viz-sv |

## Setup

1. Infrastructure: `podman compose up -d` (Postgres 17, Qdrant, RustFS + a public-read `thumbs` bucket).
2. `cp .env.example .env` and fill in the blanks (`BACKEND_API_KEY`, Telegram credentials).
3. Models: the Xenova `clip-vit-base-patch32` ONNX exports (512-dim), saved as
   ```
   models/clip-vision.onnx     <- onnx/vision_model.onnx
   models/clip-text.onnx       <- onnx/text_model.onnx
   models/clip-tokenizer.json  <- tokenizer.json
   ```
   from https://huggingface.co/Xenova/clip-vit-base-patch32. `models/` is gitignored.
4. ffmpeg on `PATH` (or `FFMPEG_BIN`); tesseract optional for OCR (`TESSERACT_BIN`).

## Ingest

```sh
cargo run --release --bin ingest -- --login                 # once, needs TG_PHONE; stores TG_SESSION_PATH
cp channels.example.toml channels.toml                      # and set CHANNELS_FILE=./channels.toml
cargo run --release --bin ingest -- --backfill @channel     # history, oldest first, resumable
cargo run --release --bin ingest                            # watch all enabled channels
cargo run --release --bin ingest -- --import-legacy memes    # one-off: old viz-rs Qdrant collection, no Telegram
```

Log in from a residential connection: Telegram often silently withholds login codes for
datacenter IPs. Copy the session file to the server afterwards; treat it like a password.

Every photo, and every scene-change frame of a video (max 40), becomes a `media_items` row
(deduplicated by dhash + frame offset) with a CLIP vector in Qdrant and a WebP thumbnail in S3.
Each post it appears in is a `media_sightings` row. Failures land in `failed_items`.

## API

All `/api/search/*` routes need the `x-api-key: $BACKEND_API_KEY` header and are rate limited
per client IP (2/s, burst 10). Behind viz-sv, the client IP is taken from `X-Forwarded-For`.

```
POST /api/search/image   multipart field "file"  [?max_distance=8, max 16]
POST /api/search/video   multipart field "file"  [?max_distance=8]
POST /api/search/text    {"q": "..."}  (1-200 chars)
GET  /healthz            {"status":"ok"}
GET  /thumbs/{key}       thumbnail WebP, public; only with API_SERVE_THUMBS=true

-> [SearchHit], at most 50, best first
SearchHit = { thumb_url, score, origin: Link | null, backup: Link | null }
Link      = { channel_id, channel_username, message_id, posted_at }
```

Image and video search return perceptual-hash matches (score = 1 - distance/64) and fall back
to CLIP similarity only when nothing matches by hash. Text search ranks caption/OCR full-text
matches (score 1.0) above CLIP text-to-image matches. Errors are `{"error": "..."}` with 400,
401, 413, 422, 429 or 500; internal details are only logged.

Thumbnails live in the S3 bucket. If the bucket is public, point `S3_PUBLIC_BASE_URL` at it.
Otherwise set `API_SERVE_THUMBS=true` (plus the `S3_*` credentials) and point
`S3_PUBLIC_BASE_URL` at `<api>/thumbs`, so the API serves them itself.

## Container

```sh
podman build -t viz-rs .
podman compose --profile app up -d     # api + ingest from that image, models mounted read-only
```

The image contains both binaries plus ffmpeg and tesseract; `api` is the default command, run
`/usr/local/bin/ingest` for the watcher.

## Tests

```sh
cargo test --workspace                      # unit tests
cargo test --workspace -- --ignored         # CLIP models + Postgres/Qdrant/RustFS from compose
```
