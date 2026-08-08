#!/usr/bin/env bash
set -euo pipefail

cd /home/runner/actions-runner

if [[ ! -f .runner ]]; then
    : "${RUNNER_TOKEN:?Set RUNNER_TOKEN to the repository registration token from GitHub Settings > Actions > Runners}"

    ./config.sh \
        --unattended \
        --replace \
        --url "${RUNNER_URL:-https://github.com/khoda81/mercy}" \
        --token "${RUNNER_TOKEN}" \
        --name "${RUNNER_NAME:-mercy-4600h}" \
        --labels "${RUNNER_LABELS:-mercy-bench}" \
        --work _work
fi

unset RUNNER_TOKEN || true
exec ./run.sh
