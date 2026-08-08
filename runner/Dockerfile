FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH=/home/runner/.cargo/bin:${PATH}

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        jq \
        build-essential \
        pkg-config \
        libssl-dev \
        libicu-dev \
        zlib1g-dev \
        libkrb5-3 \
        util-linux \
    && rm -rf /var/lib/apt/lists/*

# Ubuntu 24.04 already reserves UID 1000 in the base image. The runner lives
# entirely in Docker named volumes, so it does not need to share a numeric UID
# with the host; let useradd choose a free one.
RUN useradd --create-home --shell /bin/bash runner

WORKDIR /home/runner/actions-runner

# Keep the runner release and its published checksum together in the image
# definition. They are deliberately not runtime/.env settings: changing one
# without the other must require editing this file.
ENV RUNNER_VERSION=2.336.0
ENV RUNNER_SHA256=04cf0be1aff4c3ec3554466c39124ca250e3effd8873bb7e8d68535aa9505d5d

# Keep downloading in its own layer. If a later verification/extraction step
# fails, Docker can still reuse the expensive completed download.
RUN curl -fL \
      --retry 20 \
      --retry-all-errors \
      --retry-delay 5 \
      --connect-timeout 30 \
      -o /tmp/actions-runner.tar.gz \
      "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz"

RUN echo "${RUNNER_SHA256}  /tmp/actions-runner.tar.gz" | sha256sum -c - \
    && tar xzf /tmp/actions-runner.tar.gz \
    && rm /tmp/actions-runner.tar.gz \
    && ./bin/installdependencies.sh \
    && chown -R runner:runner /home/runner/actions-runner

USER runner

# Download rustup first, then execute it. This way a failed curl cannot be
# hidden by the exit status of a shell on the right-hand side of a pipeline.
RUN curl --proto '=https' --tlsv1.2 -fL \
      --retry 20 \
      --retry-all-errors \
      --retry-delay 5 \
      --connect-timeout 30 \
      -o /tmp/rustup-init.sh \
      https://sh.rustup.rs \
    && sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain stable \
    && rm /tmp/rustup-init.sh \
    && rustup component add rustfmt clippy

COPY --chown=runner:runner runner/entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh

ENTRYPOINT ["/home/runner/entrypoint.sh"]
