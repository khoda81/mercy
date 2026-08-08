# Isolated benchmark runner

This directory defines the canonical `mercy-bench` GitHub Actions runner.
It runs the GitHub runner application itself inside Docker rather than on the
host directly.

Security/performance choices:

- non-root user inside the container;
- no Docker socket;
- no host filesystem mounts;
- all Linux capabilities dropped;
- `no-new-privileges` enabled;
- PID and memory limits;
- pinned to one logical CPU for stable single-thread measurements;
- repository runner registration and checkout persist only in a Docker named volume.

Docker is an isolation layer, not a VM: it still shares the host kernel. The
benchmark workflow is therefore intentionally limited to manually-triggered
trusted code rather than pull requests from forks.

## Setup

1. In GitHub, open `Settings -> Actions -> Runners -> New self-hosted runner`.
2. Copy `runner/.env.example` to `runner/.env`.
3. Put the one-hour registration token shown by GitHub into `RUNNER_TOKEN`.
4. If GitHub's page shows a runner version different from the default in
   `.env.example`, set `RUNNER_VERSION` to that version.
5. Optionally inspect CPU topology with:

   ```bash
   lscpu -e=CPU,CORE,SOCKET,NODE
   ```

   and choose one logical CPU for `BENCH_CPU`.
6. Start the runner:

   ```bash
   docker compose -f runner/compose.yml --env-file runner/.env up -d --build
   ```

7. Follow startup logs:

   ```bash
   docker compose -f runner/compose.yml --env-file runner/.env logs -f
   ```

Once registration succeeds, the token is no longer needed for ordinary
restarts because the runner configuration persists in the `mercy-runner-data`
named volume.

## Stop / start

```bash
docker compose -f runner/compose.yml --env-file runner/.env stop
docker compose -f runner/compose.yml --env-file runner/.env start
```

To destroy the runner container while retaining registration/cache state:

```bash
docker compose -f runner/compose.yml --env-file runner/.env down
```

Do **not** add `-v` unless you intentionally want to delete the persistent
runner registration and checkout/build cache.
