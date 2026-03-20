FROM rust:1-trixie AS builder
WORKDIR /s3rm
COPY . ./
RUN git config --global --add safe.directory /s3rm \
&& cargo build --release

FROM debian:trixie-slim
RUN apt-get update \
&& apt-get install --no-install-recommends -y ca-certificates \
&& apt-get clean \
&& rm -rf /var/lib/apt/lists/*

COPY --from=builder /s3rm/target/release/s3rm /usr/local/bin/s3rm

RUN useradd -m -s /bin/bash s3rm
USER s3rm
WORKDIR /home/s3rm/
ENTRYPOINT ["/usr/local/bin/s3rm"]
