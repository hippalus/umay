FROM rust:slim-bookworm as builder

WORKDIR /umay

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev build-essential

COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

ENV PKG_CONFIG_ALLOW_CROSS=1
ENV OPENSSL_STATIC=1
ENV OPENSSL_DIR=/usr/local/musl
ENV DEP_OPENSSL_INCLUDE=/usr/local/musl/include
ENV TARGET_CC=musl-gcc

# Install OpenSSL, CA certificates, and other necessary tools
RUN apt-get update \
    && apt-get -y upgrade \
    && apt-get install -y openssl ca-certificates gcc musl-tools libssl-dev pkg-config cmake build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app/config /app/certs

COPY --from=builder /umay/target/release/umay /app/umay

WORKDIR /app

CMD ["./umay"]
