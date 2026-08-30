#!/usr/bin/env bash
set -euo pipefail
readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec /usr/bin/env python3 "$repo_root/scripts/ddgi_evidence/cli.py" local-terrain-convergence "$@"
