#!/usr/bin/env bash
# release.sh — publish the atd workspace to crates.io and cut the GitHub release.
#
# Executable form of docs/release-plan-v1.0.md §5 (publish wave) + §6 (tag +
# GitHub release). Version is read from the workspace Cargo.toml, so this
# script is reusable for every release on the 1.x line.
#
# PREREQUISITES — ensure these before running:
#   1. docs/release-plan-v1.0.md §4 checklist has passed (gates, schema,
#      cargo audit, cargo doc).
#   2. `cargo login` done — a crates.io token with publish-new + publish-update.
#   3. `gh auth login` done — for the GitHub release step (§6).
#   4. crates-io is NOT replaced by a mirror in ~/.cargo/config.toml. If
#      `cargo publish` errors "crates-io is replaced with ...", comment out the
#      `replace-with` line, run this script, then restore the line afterwards.
#   5. Clean git tree on the default branch, pushed to origin.
#
# USAGE:
#   scripts/release.sh --dry-run    # preflight + print the plan; publishes nothing
#   scripts/release.sh              # the real release — IRREVERSIBLE
#
# RESUMABLE: any crate already on crates.io at the target version is skipped,
# so a wave that fails partway through (network, indexing lag) re-runs cleanly.

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

# --- version from the workspace Cargo.toml ---
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[[ -n "$VERSION" ]] || die "could not read workspace version from Cargo.toml"
TAG="v${VERSION}"

# --- dependency-ordered publish waves (atd-mock-weather-server is publish=false) ---
WAVES=(
  "atd-protocol"
  "atd-runtime"
  "atd-server atd-sdk atd-middleware-fhir atd-middleware-pii-redact-medical"
  "atd-server-http atd-conformance atd-tools-echo atd-tools-fs atd-tools-shell atd-tools-web"
  "atd-cli atd-mcp-bridge atd-ref-server"
)
ALL="${WAVES[*]}"

# --- helper: is $1 already on crates.io at $VERSION? ---
published() {
  curl -fsS -A "$UA" "https://crates.io/api/v1/crates/$1/$VERSION" -o /dev/null 2>/dev/null
}

say "atd release ${TAG}   (dry-run=${DRY_RUN})"

# ---------------- preflight ----------------
say "preflight"
[[ -z "$(git status --porcelain)" ]] || die "working tree is dirty — commit or stash first"
git rev-parse "$TAG" >/dev/null 2>&1 && die "tag ${TAG} already exists locally"
command -v curl >/dev/null || die "curl is required"
command -v gh   >/dev/null || warn "gh not found — the GitHub release step (§6) will be skipped"

total=0; todo=0
for c in $ALL; do
  total=$((total + 1))
  published "$c" || todo=$((todo + 1))
done
say "  version ${VERSION} · ${total} publishable crates · ${todo} not yet on crates.io"

# ---------------- dry run ----------------
if [[ $DRY_RUN -eq 1 ]]; then
  say "dry-run plan:"
  i=0
  for wave in "${WAVES[@]}"; do
    i=$((i + 1)); echo "  wave $i:"
    for c in $wave; do
      if published "$c"; then echo "    skip     $c ${VERSION} (already on crates.io)"
      else                    echo "    PUBLISH  $c ${VERSION}"; fi
    done
  done
  say "dry-run done — nothing published. Re-run without --dry-run for the real release."
  exit 0
fi

# ---------------- confirm (irreversible) ----------------
warn "cargo publish is IRREVERSIBLE — a published version can be yanked but never deleted."
read -r -p "Publish atd ${VERSION} to crates.io and tag ${TAG}? Type 'release' to proceed: " ans
[[ "$ans" == "release" ]] || die "aborted by user"

# ---------------- publish waves (§5) ----------------
i=0
for wave in "${WAVES[@]}"; do
  i=$((i + 1))
  say "wave $i — $wave"
  for c in $wave; do
    if published "$c"; then
      echo "    skip     $c ${VERSION} (already on crates.io)"
      continue
    fi
    echo "    publish  $c ${VERSION}"
    cargo publish -p "$c"
  done
  # wait for crates.io to index this wave before the next wave depends on it
  if [[ $i -lt ${#WAVES[@]} ]]; then
    for c in $wave; do
      printf '    indexing %s ' "$c"
      for _ in $(seq 1 60); do
        if published "$c"; then echo "ok"; break; fi
        printf '.'; sleep 5
      done
      published "$c" || die "timed out waiting for $c ${VERSION} to index on crates.io"
    done
  fi
done
say "all ${total} crates published at ${VERSION}"

# ---------------- tag + GitHub release (§6) ----------------
say "tagging ${TAG}"
git tag -a "$TAG" -m "atd ${VERSION} — first stable release; wire format frozen for the 1.x line.
See CHANGELOG.md and docs/release-plan-v1.0.md."
git push origin "$TAG"

if command -v gh >/dev/null; then
  say "creating GitHub release ${TAG}"
  gh release create "$TAG" --repo "$REPO" \
    --title "atd ${VERSION} — first stable release" \
    --notes-file CHANGELOG.md --verify-tag
else
  warn "gh not installed — create the GitHub release for ${TAG} manually"
fi

say "release ${TAG} complete."
warn "if you disabled the RsProxy mirror in ~/.cargo/config.toml, restore it now."
