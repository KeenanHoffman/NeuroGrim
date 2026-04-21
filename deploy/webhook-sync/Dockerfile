# syntax=docker/dockerfile:1.6
#
# neurogrim-webhook-sync — signed-push → git-sync sidecar.
#
# Purpose: accept GitHub-style push webhooks, HMAC-verify them, and run
# `git fetch && git reset --hard` inside a shared named volume so the
# agent containers see the new HEAD. See `../README.md` for scope and
# the full remote-agent topology this fits into.
#
# The agent containers never touch git themselves — all network-fetch
# happens here. That keeps the agent's runtime image small and its
# permissions narrow.

FROM python:3.12-slim-bookworm AS runtime

# `git` is the primary tool; `ca-certificates` gets HTTPS clones working.
# `tini` is a 1-process-supervisor that reaps zombies so a flaky git
# subprocess doesn't leave orphans inside the container.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        git \
        ca-certificates \
        tini \
    && rm -rf /var/lib/apt/lists/*

# Non-root runtime per hardening guidelines. UID 1000 matches the
# pattern used by the NeuroGrim image so volume permissions line up
# when the two containers share a named workspace.
RUN groupadd --system --gid 1000 brain \
    && useradd --system --uid 1000 --gid brain --home-dir /home/brain --create-home brain

WORKDIR /app

# Install Python deps first so the application-source layer can cache.
COPY pyproject.toml /app/
RUN pip install --no-cache-dir \
        "fastapi>=0.115,<0.117" \
        "uvicorn>=0.30,<0.40" \
        "tomli>=2.0"

COPY webhook_sync/ /app/webhook_sync/

# State + config mount points.
RUN mkdir -p /data /etc/webhook-sync /workspaces \
    && chown -R brain:brain /data /etc/webhook-sync /workspaces

USER brain

# WEBHOOK_SYNC_CONFIG points at the mounted TOML; adopters override
# at `docker run` / compose time.
ENV WEBHOOK_SYNC_CONFIG=/etc/webhook-sync/config.toml

EXPOSE 4747

# tini ensures git subprocesses are reaped even if the FastAPI worker
# is misbehaving. `-g` forwards SIGTERM to the whole process group on
# `docker stop`, giving uvicorn a clean shutdown window.
ENTRYPOINT ["/usr/bin/tini", "-g", "--"]
CMD ["uvicorn", "webhook_sync.app:app", "--host", "0.0.0.0", "--port", "4747"]
