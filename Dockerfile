# One image with both binaries: `api` (default) and `ingest`.
# Models are not baked in; mount them at /app/models (see README).
FROM docker.io/library/rust:1-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev cmake clang ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY migrations migrations

# Cache mounts keep the registry and target dir between builds without putting them in a layer.
# ort's download-binaries fetches ONNX Runtime at build time.
RUN --mount=type=cache,id=viz-rs-cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=viz-rs-target,sharing=locked,target=/app/target \
    cargo build --release --locked --bin api --bin ingest \
    && mkdir -p /out \
    && cp target/release/api target/release/ingest /out/ \
    && (cp target/release/libonnxruntime*.so* /out/ 2>/dev/null || true)

# Must match the builder's Debian release (glibc).
FROM docker.io/library/debian:trixie-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3t64 ffmpeg tesseract-ocr tesseract-ocr-eng \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app viz

COPY --from=builder /out/ /usr/local/bin/
ENV LD_LIBRARY_PATH=/usr/local/bin \
    CLIP_VISION_MODEL_PATH=/app/models/clip-vision.onnx \
    CLIP_TEXT_MODEL_PATH=/app/models/clip-text.onnx \
    CLIP_TOKENIZER_PATH=/app/models/clip-tokenizer.json \
    FFMPEG_BIN=ffmpeg \
    TESSERACT_BIN=tesseract \
    TG_SESSION_PATH=/app/data/ingest.session

WORKDIR /app
RUN mkdir -p /app/data && chown viz /app/data
USER viz

EXPOSE 8080
CMD ["/usr/local/bin/api"]
