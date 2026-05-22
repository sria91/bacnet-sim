# syntax=docker/dockerfile:1
# ---------------------------------------------------------------------------
# Stage 1: Build
# ---------------------------------------------------------------------------
FROM rust:alpine AS builder

# Build dependencies for ring / openssl / tokio-signal
RUN apk add --no-cache musl-dev pkgconfig openssl-dev

WORKDIR /build
COPY . .

# Compile the release binary statically against musl
RUN cargo build --release --bin bacnet-sim

# ---------------------------------------------------------------------------
# Stage 2: Runtime image
# ---------------------------------------------------------------------------
FROM alpine:3.23

# ca-certificates needed when BACnet/SC connects to external TLS hubs
RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

COPY --from=builder /build/target/release/bacnet-sim /app/bacnet-sim

# ---------------------------------------------------------------------------
# Environment variables (all have sensible defaults for demo / single-device
# mode; set BACNET_CONFIG_FILE to switch to full config-file mode)
# ---------------------------------------------------------------------------

# ---- Logging ---------------------------------------------------------------
# BACNET_LOG_FORMAT: "text" (default) | "json"
ENV BACNET_LOG_FORMAT=text

# RUST_LOG controls the tracing filter, e.g. "info", "bacnet_sim=debug"
ENV RUST_LOG=info

# ---- Network ports ---------------------------------------------------------
# BACNET_IP_PORT: UDP port the BACnet/IP transport binds on (default: 47808)
ENV BACNET_IP_PORT=47808

# BACNET_API_PORT: TCP port the REST management API listens on (default: 8080)
ENV BACNET_API_PORT=8080

# BACNET_SC_PORT: TCP port the BACnet/SC WebSocket hub listens on (default: 47814)
# Only used when a network entry with transport = "bacnet_sc" is present.
ENV BACNET_SC_PORT=47814

# ---- Demo-mode device ------------------------------------------------------
# BACNET_DEVICE_ID: BACnet device instance number used in demo mode (default: 1234)
ENV BACNET_DEVICE_ID=1234

# BACNET_TICK_HZ: Simulation tick rate in Hz for demo mode (default: 1.0)
ENV BACNET_TICK_HZ=1.0

# ---- Config-file mode ------------------------------------------------------
# BACNET_CONFIG_FILE: absolute path to a TOML topology file mounted into the
# container.  When set, demo-mode is skipped and all network/device/profile
# definitions come from the file.
# Example: -e BACNET_CONFIG_FILE=/config/topology.toml \
#          -v ./topology.toml:/config/topology.toml:ro
ENV BACNET_CONFIG_FILE=

# ---------------------------------------------------------------------------
# Expose the standard BACnet/IP UDP port, the REST API TCP port,
# and the BACnet/SC WebSocket TCP port.
# ---------------------------------------------------------------------------
EXPOSE ${BACNET_IP_PORT}/udp
EXPOSE ${BACNET_API_PORT}/tcp
EXPOSE ${BACNET_SC_PORT}/tcp

# ---------------------------------------------------------------------------
# Entry point — all configuration is read from environment variables.
# Additional CLI flags (e.g. --log-format=json) can be passed as CMD args.
# ---------------------------------------------------------------------------
ENTRYPOINT ["/app/bacnet-sim"]
