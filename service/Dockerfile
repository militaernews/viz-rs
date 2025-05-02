# Step 1: Base image for cargo-chef and Rust toolchain
FROM lukemathwalker/cargo-chef:latest-rust-1.86.0 AS chef
WORKDIR /app

# Step 2: Plan build using cargo-chef
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Step 3: Install dependencies, libtorch, and build app
FROM chef AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl unzip cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Download and extract libtorch (CPU version)
ENV LIBTORCH_VERSION=2.7.0
ENV LIBTORCH_DOWNLOAD_URL=https://download.pytorch.org/libtorch/cpu/libtorch-cxx11-abi-shared-with-deps-${LIBTORCH_VERSION}%2Bcpu.zip
RUN curl -L "${LIBTORCH_DOWNLOAD_URL}" -o libtorch.zip \
    && unzip libtorch.zip -d /opt \
    && rm libtorch.zip

# Set LIBTORCH environment variables for build
ENV LIBTORCH=/opt/libtorch
ENV LD_LIBRARY_PATH=/opt/libtorch/lib

WORKDIR /app

# Copy recipe and cook dependencies
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY . .

# Build with libtorch
RUN LIBTORCH=$LIBTORCH \
    LD_LIBRARY_PATH=$LD_LIBRARY_PATH \
    cargo build --release --bin viz-rs

# Step 4: Runtime container with only what's needed to run
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl-dev ca-certificates libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Copy libtorch shared libraries
COPY --from=builder /opt/libtorch /opt/libtorch

# Set runtime environment variables
ENV LD_LIBRARY_PATH=/opt/libtorch/lib

# Copy binary
COPY --from=builder /app/target/release/viz-rs /usr/local/bin

# Expose the port
EXPOSE 3000

# Run the application
CMD ["/usr/local/bin/viz-rs"]