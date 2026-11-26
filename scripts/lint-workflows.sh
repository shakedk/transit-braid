#!/usr/bin/env bash
# Lints every GitHub Actions workflow, composite action and dependabot.yml in the repo with
# zizmor, the version scripts/install-tools.sh pins.
# Usage: scripts/lint-workflows.sh            (pnpm lint:workflows; part of pnpm check)
#   Offline: no network and no token, so only the audits that read the files run.
# Usage: scripts/lint-workflows.sh --online   (CI's workflow lint job)
#   Also the audits that ask GitHub: a pinned commit that isn't in the action's own repository
#   (impostor commits), a version comment that doesn't match its pin, known-vulnerable action
#   versions. Needs a token in GH_TOKEN: CI gives the job's read-only GITHUB_TOKEN.
# Without zizmor it skips with a note on a developer's machine, and fails in CI.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PINNED=$(sed -n 's/^ZIZMOR_VERSION=//p' "$ROOT/scripts/install-tools.sh")

online=false
if [[ "${1:-}" == "--online" ]]; then
  online=true
  shift
fi
if [[ $# -gt 0 ]]; then
  echo "usage: scripts/lint-workflows.sh [--online]" >&2
  exit 2
fi

if ! command -v zizmor >/dev/null 2>&1; then
  if [[ -n "${CI:-}" ]]; then
    echo "zizmor is missing: run scripts/install-tools.sh zizmor" >&2
    exit 1
  fi
  echo "zizmor is not installed, so the workflow lint is skipped (pnpm setup:tools installs it)."
  exit 0
fi
found=$(zizmor --version)
if [[ "$found" != "zizmor $PINNED" ]]; then
  echo "Found $found but scripts/install-tools.sh pins zizmor $PINNED: run pnpm setup:tools." >&2
  exit 1
fi

cd "$ROOT"
# Everything git doesn't ignore; a file that isn't valid YAML or breaks the schema fails too.
if [[ "$online" == true ]]; then
  if [[ -z "${GH_TOKEN:-}" ]]; then
    echo "--online needs a GitHub token in GH_TOKEN (in CI: the job's GITHUB_TOKEN)." >&2
    exit 1
  fi
  # zizmor reads the token from GH_TOKEN; any finding fails, as offline.
  exec zizmor --strict-collection --no-progress .
fi
exec zizmor --offline --strict-collection --no-progress .
