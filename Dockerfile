#  Builder part
FROM debian:bookworm-slim AS builder

# Set environment variables to avoid interaction during installation
ENV DEBIAN_FRONTEND=noninteractive

# Add the Bookworm Backports repository
RUN echo "deb http://deb.debian.org/debian bookworm-backports main" > /etc/apt/sources.list.d/bookworm-backports.list \
    && apt-get update

# Update and install required packages
RUN apt-get -t bookworm-backports install -y libreofficekit-dev

# Update and install required packages
RUN apt-get install -y clang curl build-essential libssl-dev pkg-config ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Add rust target and install deps
# RUN rustup target add x86_64-unknown-linux-musl
RUN update-ca-certificates

ENV LO_INCLUDE_PATH=/usr/include/LibreOfficeKit
ENV PATH="/root/.cargo/bin:${PATH}"


WORKDIR /app

# Dependency precachng
COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() {}" >src/main.rs
RUN cargo build --target x86_64-unknown-linux-gnu --release

COPY src src
RUN touch src/main.rs

RUN cargo build --target x86_64-unknown-linux-gnu --release

# ----------------------------------------
# Runner part
# ----------------------------------------
FROM debian:bookworm-slim AS runner

# Set environment variables to avoid interaction during installation
ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /app

# Install dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends libreoffice && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/office-convert-server ./

ENV LIBREOFFICE_SDK_PATH=/usr/lib/libreoffice/program
ENV SERVER_ADDRESS=0.0.0.0:3000 

EXPOSE 8080

CMD ["/app/office-convert-server"]
