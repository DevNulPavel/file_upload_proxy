# Сборка с помощью пакета Rust
# https://hub.docker.com/_/rust
FROM rust:1.56.1 as builder
WORKDIR /usr/src/file_upload_proxy
COPY ./prod_deploy_test/ ./prod_deploy_test/
COPY ./src/ ./src/
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./.cargo ./.cargo
RUN \
    ls -la && \
    cargo build --release

# Сборка рабочего пакета
FROM debian:11.1
RUN \
    apt-get update && \
    apt-get install -y ca-certificates && \
    update-ca-certificates
WORKDIR /file_upload_proxy
COPY --from=builder \
    /usr/src/file_upload_proxy/target/release/file_upload_proxy \
    file_upload_proxy
CMD ["./file_upload_proxy"]