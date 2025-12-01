FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install Python, FUSE, and build dependencies
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-venv \
    libfuse3-dev \
    fuse3 \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Allow FUSE to run as non-root (needed in container)
RUN echo 'user_allow_other' >> /etc/fuse.conf

# Set up Rust components
RUN rustup component add rustfmt clippy rust-analyzer