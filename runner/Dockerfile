FROM ubuntu:24.04

ARG RUNNER_VERSION=2.334.0

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

RUN curl -fsSL \
      "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
      | tar xz \
    && ./bin/installdependencies.sh \
    && chown -R runner:runner /home/runner/actions-runner

USER runner

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup component add rustfmt clippy

COPY --chown=runner:runner runner/entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh

ENTRYPOINT ["/home/runner/entrypoint.sh"]
