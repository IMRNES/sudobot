# The dashboard SPA, built and then served by nginx.
#
# The development story is Vite's dev server; this image is for a deployment where
# the frontend and the API share a host, which is also what makes cookies work
# without a CORS dance.
#
# The build context is the repository root, because pnpm needs the workspace
# manifests and the lockfile that live there:
#
#   docker build -f web/Dockerfile -t sudobot-web .

# ---------------------------------------------------------------------------
# Build stage
# ---------------------------------------------------------------------------
FROM node:24-slim AS builder

# Pin pnpm to the version the lockfile was written with, so `--frozen-lockfile`
# means something.
RUN corepack enable && corepack prepare pnpm@10.33.2 --activate

WORKDIR /build

# Manifests and the lockfile first, so a source edit does not reinstall the tree.
# There is no root `package.json` in this workspace — `pnpm-workspace.yaml` is what
# declares it — so the list is exactly what exists.
COPY pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY web/package.json web/

RUN pnpm install --frozen-lockfile

COPY web web

# Baked into the bundle at build time — Vite inlines `VITE_*` during the build, so
# this must be the URL the browser will use, not the one the container can reach.
# Empty means same-origin, which is what the nginx proxy below provides.
ARG VITE_API_BASE_URL=""
ENV VITE_API_BASE_URL=${VITE_API_BASE_URL}

RUN pnpm --filter web build

# ---------------------------------------------------------------------------
# Runtime stage
# ---------------------------------------------------------------------------
FROM nginx:1.27-alpine AS runtime

COPY --from=builder /build/web/dist /usr/share/nginx/html
COPY web/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
    CMD wget -qO- http://127.0.0.1/ >/dev/null || exit 1

CMD ["nginx", "-g", "daemon off;"]
