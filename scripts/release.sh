#!/usr/bin/env bash
# release.sh — publish the atd workspace to crates.io and cut the GitHub release.
#
# Executable form of docs/release-plan-v1.0.md §5 (publish wave) + §6 (tag +
# GitHub release). Per ADR 0004 the workspace ships **per-crate independent
# SemVer** anchored on `atd-protocol`'s version for the ATD release identity:
# each crate's Cargo.toml carries its own `version = "X.Y.Z"`; this script
# publishes any crate whose declared version is not yet on crates.io, and
# cuts a top-level `v<atd-protocol-version>` tag + GitHub release only when
# atd-protocol's version has bumped (i.e. the tag doesn't already exist).
#
# PREREQUISITES — ensure these before running:
#   1. docs/release-plan-v1.0.md §4 checklist has passed (gates, schema,
#      cargo audit, cargo doc).
#   2. `cargo login` done — a crates.io token with publish-new + publish-update.
#   3. `gh auth login` done — for the GitHub release step (§6).
#   4. Each crate you intend to publish has its `version = "X.Y.Z"` bumped
#      to a value not yet on crates.io. Unbumped crates are skipped
#      automatically; nothing to do if you don't want to ship them.
#   5. Clean git tree on the default branch, pushed to origin.
#
# USAGE:
#   scripts/release.sh --dry-run    # preflight + print the plan; publishes nothing
#   scripts/release.sh              # the real release — IRREVERSIBLE
#
# RESUMABLE: any crate already on crates.io at its declared version is
# skipped, so a wave that fails partway through (network, indexing lag)
# re-runs cleanly.

set -euo pipefail
cd "$(dirname "$0")/.."

REPO="downsea/atd"
UA="atd-release-script (https://github.com/downsea/atd)"

DRY_RUN=0
case "${1:-}" in
  --dry-run) DRY_RUN=1 ;;
  "")        DRY_RUN=0 ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac

grn=$'\033[0;32m'; yel=$'\033[0;33m'; red=$'\033[0;31m'; rst=$'\033[0m'
say()  { printf '%s==>%s %s\n' "$grn" "$rst" "$*"; }
warn() { printf '%s !  %s%s\n'  "$yel" "$*" "$rst"; }
die()  { printf '%sERROR: %s%s\n' "$red" "$*" "$rst" >&2; exit 1; }

# --- crate_version CRATE — reads the explicit `version = "X.Y.Z"` from
#     `crates/<CRATE>/Cargo.toml`. Per ADR 0004 each crate carries its own
#     version line; nothing is inherited from `[workspace.package]`.
crate_version() {
  local c="$1"
  local v
  v=$(sed -n 's/^version = "\(.*\)"/\1/p' "crates/$c/Cargo.toml" | head -1)
  [[ -n "$v" ]] || die "could not read version from crates/$c/Cargo.toml"
  printf '%s\n' "$v"
}

# --- dependency-ordered publish waves (atd-mock-weather-server is publish=false) ---
WAVES=(
  "atd-protocol"
  "atd-runtime"
  "atd-server atd-sdk atd-middleware-fhir atd-middleware-pii-redact-medical"
  "atd-server-http atd-conformance atd-tools-echo atd-tools-fs atd-tools-shell atd-tools-web"
  "atd-cli atd-mcp-bridge atd-ref-server"
)
ALL="${WAVES[*]}"

# --- ATD release identity — anchored on atd-protocol's version ---
ATD_VERSION=$(crate_version atd-protocol)
TAG="v${ATD_VERSION}"

# --- helper: is crate $1 already on crates.io at version $2? ---
published_at() {
  curl -fsS -A "$UA" "https://crates.io/api/v1/crates/$1/$2" -o /dev/null 2>/dev/null
}

say "atd release ${TAG}   (dry-run=${DRY_RUN})"

# ---------------- preflight ----------------
say "preflight"
[[ -z "$(git status --porcelain)" ]] || die "working tree is dirty — commit or stash first"
command -v curl >/dev/null || die "curl is required"
command -v gh   >/dev/null || warn "gh not found — the GitHub release step (§6) will be skipped"

# Per ADR 0004 the top-level tag only fires when atd-protocol bumps. If the
# tag for the current atd-protocol version already exists, we're doing a
# between-release publish (one or more non-protocol crates bumped) — the
# tagging / GH release section is skipped at the end.
TAG_EXISTS=0
if git rev-parse "$TAG" >/dev/null 2>&1; then
  TAG_EXISTS=1
  warn "tag ${TAG} already exists — this is a between-release publish (atd-protocol unchanged)"
fi

total=0; todo=0
declare -A PENDING   # crate -> version pairs needing publish
declare -A VERSIONS  # crate -> version (for reporting)
for c in $ALL; do
  total=$((total + 1))
  v=$(crate_version "$c")
  VERSIONS["$c"]="$v"
  if ! published_at "$c" "$v"; then
    PENDING["$c"]="$v"
    todo=$((todo + 1))
  fi
done
say "  ATD release ${TAG} · ${total} publishable crates · ${todo} not yet on crates.io"

# ---------------- dry run ----------------
if [[ $DRY_RUN -eq 1 ]]; then
  say "dry-run plan:"
  i=0
  for wave in "${WAVES[@]}"; do
    i=$((i + 1)); echo "  wave $i:"
    for c in $wave; do
      v="${VERSIONS[$c]}"
      if [[ -n "${PENDING[$c]:-}" ]]; then
        echo "    PUBLISH  $c $v"
      else
        echo "    skip     $c $v (already on crates.io)"
      fi
    done
  done
  if [[ $TAG_EXISTS -eq 1 ]]; then
    echo "  tagging:  skip (${TAG} already exists; atd-protocol version unchanged)"
  else
    echo "  tagging:  ${TAG} (new) + GitHub release"
  fi
  say "dry-run done — nothing published. Re-run without --dry-run for the real release."
  exit 0
fi

# Nothing pending? Bail out cleanly — keeps `scripts/release.sh` safe to
# run as a no-op smoke check.
if (( todo == 0 )); then
  say "nothing to publish — all ${total} crates already on crates.io at their declared versions."
  if [[ $TAG_EXISTS -eq 1 ]]; then
    say "tag ${TAG} already exists. Done."
    exit 0
  fi
fi

# ---------------- confirm (irreversible) ----------------
if (( todo > 0 )); then
  if (( todo < total )); then
    say "resuming — $(( total - todo )) crate(s) already on crates.io; publishing the remaining ${todo}"
  else
    warn "cargo publish is IRREVERSIBLE — a published version can be yanked but never deleted."
    read -r -p "Publish ${todo} crate(s) to crates.io? Type 'release' to proceed: " ans
    [[ "$ans" == "release" ]] || die "aborted by user"
  fi
fi

# publish_one CRATE — cargo publish, transparently waiting out crates.io 429
# rate limits: on a 429, sleep until the server's retry-after time and retry
# the same crate. Any other failure is fatal. crates.io throttles *new*
# crates (~1 per 10 min after a burst), so a 15-crate first release sleeps
# through several of these; the script resumes each crate by itself.
publish_one() {
  local c="$1" log rc after target now secs
  log=$(mktemp)
  while true; do
    # --registry crates-io forces publish to crates.io even when the host
    # has `[source.crates-io] replace-with = "<mirror>"` configured (e.g.
    # RsProxy on the Chinese mainland). Without this flag cargo errors
    # "crates-io is replaced with ..." and the publish never starts.
    if cargo publish --registry crates-io -p "$c" 2>&1 | tee "$log"; then rm -f "$log"; return 0; fi
    rc=${PIPESTATUS[0]:-1}
    if grep -q '429 Too Many Requests' "$log"; then
      after=$(sed -n 's/.*try again after \(.*GMT\).*/\1/p' "$log" | head -1)
      secs=600
      if [[ -n "$after" ]]; then
        target=$(date -d "$after" +%s 2>/dev/null || echo 0)
        now=$(date +%s)
        if (( target > now )); then secs=$(( target - now + 20 )); fi
      fi
      warn "crates.io rate-limit hit — waiting ${secs}s, then retrying ${c}"
      sleep "$secs"
      continue
    fi
    rm -f "$log"
    return "$rc"
  done
}

# ---------------- publish waves (§5) ----------------
i=0
for wave in "${WAVES[@]}"; do
  i=$((i + 1))
  say "wave $i — $wave"
  for c in $wave; do
    v="${VERSIONS[$c]}"
    if [[ -z "${PENDING[$c]:-}" ]]; then
      echo "    skip     $c $v (already on crates.io)"
      continue
    fi
    echo "    publish  $c $v"
    publish_one "$c"
  done
  # Wait for crates.io to index this wave before the next wave depends on
  # it — but only for crates we just published. Already-indexed crates are
  # skipped.
  if [[ $i -lt ${#WAVES[@]} ]]; then
    for c in $wave; do
      [[ -n "${PENDING[$c]:-}" ]] || continue
      v="${VERSIONS[$c]}"
      printf '    indexing %s ' "$c"
      for _ in $(seq 1 60); do
        if published_at "$c" "$v"; then echo "ok"; break; fi
        printf '.'; sleep 5
      done
      published_at "$c" "$v" || die "timed out waiting for $c $v to index on crates.io"
    done
  fi
done
say "publish complete — ${todo} crate(s) shipped this run"

# ---------------- tag + GitHub release (§6) ----------------
# Tag/release only when atd-protocol's version is new — i.e. this is the
# release that establishes the ATD release identity. Between-release
# publishes (only non-protocol crates changed) skip this section.
if [[ $TAG_EXISTS -eq 1 ]]; then
  say "tag ${TAG} pre-existed — skipping tag + GitHub release (between-release publish)"
else
  say "tagging ${TAG}"
  # Tag message keeps the canonical "ATD release identity = atd-protocol
  # version" framing per ADR 0004. The 1.x stability contract from
  # docs/release-plan-v1.0.md continues to apply to the wire (atd-protocol).
  git tag -a "$TAG" -m "atd ${ATD_VERSION} — release of the ATD wire (atd-protocol ${ATD_VERSION}).
See CHANGELOG.md for the full changeset; docs/release-plan-v1.0.md for the 1.x stability contract."
  git push origin "$TAG"

  if command -v gh >/dev/null; then
    say "creating GitHub release ${TAG}"
    gh release create "$TAG" --repo "$REPO" \
      --title "atd ${ATD_VERSION}" \
      --notes-file CHANGELOG.md --verify-tag
  else
    warn "gh not installed — create the GitHub release for ${TAG} manually"
  fi
fi

say "release ${TAG} complete."
