FROM debian:trixie AS builder
WORKDIR /s3rm
COPY . ./
RUN apt-get update \
&& apt-get install --no-install-recommends -y ca-certificates curl build-essential \
&& curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs >./rust_install.sh \
&& chmod +x rust_install.sh \
&& ./rust_install.sh -y \
&& . "$HOME/.cargo/env" \
&& rustup update \
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
