# Step 1: Base image for cargo-chef and Rust toolchain
FROM lukemathwalker/cargo-chef:latest-rust-1.90.0 AS chef
WORKDIR /app




# Step 2: Plan build using cargo-chef (unchanged)
FROM chef AS planner
COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# Step 3: Install dependencies, libtorch, and build app
FROM chef AS builder

# Install system dependencies more efficiently
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl unzip cmake pkg-config libssl-dev \
    clang lld \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*




# Cache libtorch in a separate layer
ENV LIBTORCH_VERSION=2.7.0
ENV LIBTORCH_DOWNLOAD_URL=https://download.pytorch.org/libtorch/cpu/libtorch-cxx11-abi-shared-with-deps-${LIBTORCH_VERSION}%2Bcpu.zip

# Download and extract libtorch in a separate layer for better caching
RUN curl -L "${LIBTORCH_DOWNLOAD_URL}" -o libtorch.zip \
    && unzip libtorch.zip -d /opt \
    && rm libtorch.zip

# Set LIBTORCH environment variables for build
ENV LIBTORCH=/opt/libtorch
ENV LD_LIBRARY_PATH=/opt/libtorch/lib

WORKDIR /app

# Copy recipe and cook dependencies
COPY --from=planner /app/recipe.json recipe.json
# Use release mode with optimizations
RUN cargo chef cook --release --recipe-path recipe.json



ENV DATABASE_URL="postgresql://nm_owner:SLf4nwQEsPc1@ep-wandering-scene-a25ox7k0.eu-central-1.aws.neon.tech/nm?sslmode=require"

# Copy source code
COPY . .

# Build with libtorch and all optimizations
#  RUSTC_WRAPPER=sccache \ # Uncomment if sccache is installed
RUN LIBTORCH=$LIBTORCH \
     DATABASE_URL=$DATABASE_URL \
    LD_LIBRARY_PATH=$LD_LIBRARY_PATH \
    cargo build --bin viz-rs
#--release

# Step 4: Runtime container with only what's needed to run (unchanged)
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update

RUN apt-get install -y --no-install-recommends  libssl-dev ca-certificates libgomp1

RUN apt-get clean

RUN rm -rf /var/lib/apt/lists/*



# Copy libtorch shared libraries
COPY --from=builder /opt/libtorch /opt/libtorch

# Set runtime environment variables
ENV LD_LIBRARY_PATH=/opt/libtorch/lib

# Copy binary
COPY --from=builder /app/target/debug/viz-rs /usr/local/bin
# change to  "release"

# Expose the port
EXPOSE 3000

# Run the application
CMD ["/usr/local/bin/viz-rs"]