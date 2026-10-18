#!/usr/bin/env bash
#
# deploy-direct.sh — run ON the target host by CI, never by hand.
#
#   ARTIFACT_DIR=/tmp/sudobot-payload SYSTEMD_SRC=/tmp/sudobot-systemd \
#     bash deploy-direct.sh <sha>
#
# The compile happens in GitHub Actions and only the resulting binaries plus the
# dashboard bundle are shipped, because the alternative — cloning the repo onto
# the host and building there — costs ~700 MB of `target/` per release for four
# executables, and needs a Rust toolchain on a box that otherwise has none.
#
# Layout, following the convention the other services on this host already use:
#
#   /opt/sudobot/releases/<sha>/{bin,systemd,web}
#   /opt/sudobot/current -> releases/<sha>
#
# `current` is a symlink so a rollback is one `ln -sfn` and no rebuild.
#
# Secrets come from Bitwarden Secrets Manager through `/usr/local/bin/bws-exec`,
# in each unit's ExecStart, as `<app>_<KEY>` keys. There is deliberately no
# `EnvironmentFile=`: systemd loads those BEFORE ExecStartPre runs, so a file
# written by a pre-step is not there yet when systemd reads it. The wrapper
# fetches, exports and execs in one process, which is the ordering that works.

set -euo pipefail

SHA="${1:-}"
if [[ -z "$SHA" ]]; then
  echo "usage: deploy-direct.sh <sha>" >&2
  exit 2
fi

APP=sudobot
ROOT="/opt/$APP"
RELEASES="$ROOT/releases"
RELEASE="$RELEASES/$SHA"
CURRENT="$ROOT/current"
SERVICE_USER="$APP"

# How many releases to keep in TOTAL on disk: the live one plus one rollback.
# Two is the smallest number that can still roll back.
KEEP_RELEASES="${KEEP_RELEASES:-2}"

ARTIFACT_DIR="${ARTIFACT_DIR:-}"
# The unit files travel with the code, so a unit installed by hand on one host is
# not a dependency. This is the CI checkout, not the live release, because a fix
# to the deploy script or a unit could otherwise never take effect — the copy that
# would apply it is itself the stale one.
SYSTEMD_SRC="${SYSTEMD_SRC:-/tmp/sudobot-systemd}"

CADDYFILE=/etc/caddy/Caddyfile
# The proxy block is managed as a marked region inside the existing Caddyfile.
# There is no `import conf.d/*` on this host, so dropping a file in a directory
# would be silently unreachable config that still looks installed.
CADDY_BEGIN="# >>> sudobot (managed by CI — do not edit by hand) >>>"
CADDY_END="# <<< sudobot <<<"

log() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { printf 'deploy: %s\n' "$*" >&2; exit 1; }

[[ "$(id -u)" == "0" ]] || die "must run as root"
[[ -n "$ARTIFACT_DIR" ]] || die "ARTIFACT_DIR is required (CI ships the binaries; this host has no toolchain)"
[[ -d "$SYSTEMD_SRC" ]] || die "SYSTEMD_SRC $SYSTEMD_SRC not found (CI copies the unit files there)"

# ---------------------------------------------------------------------------
# Payload audit — before anything is stopped or overwritten.
#
# The four binaries plus the dashboard are the whole payload. Asserting each one
# here means a build that produced fewer fails while the old release is still
# serving, instead of after the symlink has moved.
# ---------------------------------------------------------------------------
log "auditing the payload at $ARTIFACT_DIR"
for bin in gateway bot migrate bootstrap; do
  [[ -x "$ARTIFACT_DIR/$bin" ]] || die "payload is missing an executable: $bin"
done

[[ -f "$ARTIFACT_DIR/web/index.html" ]] || die "payload is missing the dashboard build (web/index.html)"

# A dynamically linked binary is portable in one direction only: an older build
# host targeting a newer one is fine, the reverse is not. Fail here rather than as
# an execve error after the cutover.
for bin in gateway bot migrate bootstrap; do
  missing=$(ldd "$ARTIFACT_DIR/$bin" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')
  [[ -z "$missing" ]] || die "$bin has unresolved shared libraries: $missing"
done

# ---------------------------------------------------------------------------
# The service account.
# ---------------------------------------------------------------------------
if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
  log "creating the $SERVICE_USER service account"
  useradd --system --create-home --home-dir "$ROOT" --shell /usr/sbin/nologin "$SERVICE_USER"
fi
# Group membership on /etc/bws-token is what lets the service account read the BWS
# token without that file being world-readable. Absent group is not fatal: an
# operator may be exporting BWS_ACCESS_TOKEN into the unit instead.
usermod -aG bws "$SERVICE_USER" 2>/dev/null || true

# ---------------------------------------------------------------------------
# The release.
# ---------------------------------------------------------------------------
if [[ -d "$RELEASE" ]]; then
  log "release $SHA already exists on disk; rebuilding it in place"
  rm -rf "$RELEASE"
fi
install -d -o "$SERVICE_USER" -g "$SERVICE_USER" "$RELEASE"

install -d -m 0755 "$RELEASE/bin"
for bin in gateway bot migrate bootstrap; do
  install -m 0755 -o "$SERVICE_USER" -g "$SERVICE_USER" "$ARTIFACT_DIR/$bin" "$RELEASE/bin/$bin"
done

# The dashboard is static; Caddy serves it directly from disk, so it must be
# world-readable rather than owned by the service account.
install -d -m 0755 "$RELEASE/web"
cp -r "$ARTIFACT_DIR/web/." "$RELEASE/web/"
chmod -R a+rX "$RELEASE/web"

install -d -m 0755 "$RELEASE/systemd"
cp "$SYSTEMD_SRC"/*.service "$RELEASE/systemd/"
cp "$SYSTEMD_SRC"/sudobot.caddy "$RELEASE/systemd/" 2>/dev/null || true

log "payload size on disk: $(du -sh "$RELEASE" | cut -f1)"

# ---------------------------------------------------------------------------
# Migrations and seeding. Run through bws-exec so the migrator reads the SAME
# secret the service does; assembling the URL into a shell variable and passing
# it along would be a second chance for the two to disagree, and would put the
# resolved value into the transcript.
# ---------------------------------------------------------------------------
log "applying migrations"
sudo -u "$SERVICE_USER" --preserve-env=BWS_ACCESS_TOKEN \
  /usr/local/bin/bws-exec "$APP" "$RELEASE/bin/migrate" up

log "seeding the permission catalog"
sudo -u "$SERVICE_USER" --preserve-env=BWS_ACCESS_TOKEN \
  /usr/local/bin/bws-exec "$APP" "$RELEASE/bin/bootstrap"

# ---------------------------------------------------------------------------
# Units, installed from the release so host state is never the source of truth.
# ---------------------------------------------------------------------------
log "installing systemd units"
for unit in "$RELEASE/systemd"/*.service; do
  install -m 0644 "$unit" "/etc/systemd/system/$(basename "$unit")"
done

# ---------------------------------------------------------------------------
# The reverse proxy. Replaced between markers so re-running the deploy is
# idempotent and the rest of the operator's Caddyfile is never touched.
# ---------------------------------------------------------------------------
if [[ -f "$RELEASE/systemd/sudobot.caddy" ]]; then
  log "syncing the caddy site block"
  cp "$CADDYFILE" "$CADDYFILE.bak-sudobot"

  # Everything outside the marked region is preserved verbatim; only the region
  # between the markers is regenerated. `awk` rather than `sed` because the
  # markers are multi-line boundaries, not single lines to substitute.
  tmp=$(mktemp)
  awk -v begin="$CADDY_BEGIN" -v end="$CADDY_END" '
    $0 == begin { skip = 1; next }
    skip && $0 == end { skip = 0; next }
    !skip { print }
  ' "$CADDYFILE" > "$tmp"

  {
    cat "$tmp"
    echo
    echo "$CADDY_BEGIN"
    cat "$RELEASE/systemd/sudobot.caddy"
    echo "$CADDY_END"
  } > "$CADDYFILE.new"

  mv "$CADDYFILE.new" "$CADDYFILE"
  rm -f "$tmp"

  caddy validate --config "$CADDYFILE" >/dev/null 2>&1 || {
    cp "$CADDYFILE.bak-sudobot" "$CADDYFILE"
    die "the generated caddy config is invalid; restored the previous one"
  }
fi

# ---------------------------------------------------------------------------
# Cut over.
# ---------------------------------------------------------------------------
PREVIOUS=""
if [[ -L "$CURRENT" ]]; then
  PREVIOUS="$(readlink -f "$CURRENT")"
  log "previous release: $PREVIOUS"
fi

ln -sfn "$RELEASE" "$CURRENT"

systemctl daemon-reload
log "restarting services"
systemctl enable sudobot-gateway sudobot-bot >/dev/null 2>&1 || true
systemctl restart sudobot-gateway sudobot-bot

if [[ -f "$RELEASE/systemd/sudobot.caddy" ]]; then
  systemctl reload caddy 2>/dev/null || systemctl restart caddy
fi

# ---------------------------------------------------------------------------
# Health check. `is-active` proves systemd started something, not that the
# service works, so the API is probed over HTTP as well. curl's own retry loop is
# the wait — blocking on a real process rather than polling on a timer.
# ---------------------------------------------------------------------------
log "waiting for the API to answer"
api_healthy=0
if curl -fsS --max-time 3 \
  --retry 20 --retry-delay 1 --retry-connrefused --retry-all-errors \
  http://127.0.0.1:8080/api/v1/iam/health >/dev/null 2>&1; then
  api_healthy=1
fi

bot_state=$(systemctl is-active sudobot-bot 2>/dev/null || echo inactive)
api_state=$(systemctl is-active sudobot-gateway 2>/dev/null || echo inactive)
log "  sudobot-gateway -> $api_state (http healthy: $api_healthy)"
log "  sudobot-bot     -> $bot_state"

if [[ "$api_state" != "active" || "$api_healthy" != "1" || "$bot_state" != "active" ]]; then
  log "a unit is not healthy; recent logs follow"
  journalctl -u sudobot-gateway -u sudobot-bot --no-pager --lines=40 || true

  # Roll back only when there is something to roll back TO. On a first deploy
  # there is no previous release, and the two units fail for different reasons —
  # rolling the bot back because the API lacks an OAuth secret would take down a
  # working service to fix an unrelated one.
  if [[ -z "$PREVIOUS" ]]; then
    cat >&2 <<EOF

first deploy finished with a unit down and no previous release to restore.
The release is installed at $RELEASE and stays in place so the logs and the
binaries can be inspected. Fix the cause, then re-run the deploy; nothing is
half-applied, because the failing unit never served traffic.
EOF
    exit 1
  fi

  log "rolling back to $PREVIOUS"
  ln -sfn "$PREVIOUS" "$CURRENT"
  systemctl restart sudobot-gateway sudobot-bot || true
  die "deploy failed its health check"
fi

# A restart is not proof the gateway connected: a revoked token or a refused
# privileged intent leaves the unit `active` while the bot does nothing. The log
# line the bot writes once it is really online is the check that catches that.
log "checking that the bot reached discord"
if journalctl -u sudobot-bot --since '3 min ago' --no-pager 2>/dev/null | grep -q 'connected to discord'; then
  log "  bot is connected"
else
  log "  WARNING: no 'connected to discord' line in the bot's recent log — check DISCORD_BOT_TOKEN and the privileged intents"
fi

# ---------------------------------------------------------------------------
# Retention. Pruning is by explicit path from the release list, never a glob from
# $ROOT: `current` is a symlink here, and a service whose `current` is a real
# directory loses it to a naive prune.
# ---------------------------------------------------------------------------
live="$(readlink -f "$CURRENT")"
kept=0
while IFS= read -r release; do
  if [[ "$release" == "$live" || "$release" == "$PREVIOUS" ]]; then
    kept=$((kept + 1))
    continue
  fi
  kept=$((kept + 1))
  if (( kept > KEEP_RELEASES )); then
    log "pruning $release"
    rm -rf "$release"
  fi
done < <(find "$RELEASES" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' | sort -rn | cut -d' ' -f2-)

log "deploy complete: $SHA is live at $live"
