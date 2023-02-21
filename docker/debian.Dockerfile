FROM rust:slim-bullseye

RUN set -eux; \
    apt update; \
    apt -y install libsqlite3-dev curl gzip; \
    cargo install versatiles; \
    rm -r /usr/local/cargo/registry; \
    rm -r /usr/local/rustup
