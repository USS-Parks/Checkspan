#!/usr/bin/env bash
# CS-07 evidence capture: run every example through the checkspan binary and
# record the exit code, validity, rejecting stage, and output digest of each.
#
# Usage: run-examples.sh <path-to-checkspan-binary> <output-file>
# Run from anywhere inside the repository. Needs bash, git, and sha256sum.
set -u
BIN="${1:?path to checkspan binary}"
OUT="${2:?output file}"
cd "$(git rev-parse --show-toplevel)" || exit 2
{
  echo "binary: $BIN"
  echo "version: $("$BIN" --version)"
  echo "commit: $(git rev-parse HEAD)"
  echo "tree-clean: $(if [ -z "$(git status --porcelain --untracked-files=no)" ]; then echo yes; else echo no; fi)"
  echo "platform: $(uname -srm)"
  for f in examples/graphs/*.json examples/records/*.json examples/invalid/*.json; do
    for cmd in validate inspect; do
      out="$("$BIN" "$cmd" "$f")"; code=$?
      digest="$(printf '%s\n' "$out" | sha256sum | cut -d' ' -f1)"
      valid="$(printf '%s' "$out" | grep -o '"valid": [a-z]*' | head -1)"
      stage="$(printf '%s' "$out" | grep -o '"stage": "[a-z_]*"' | head -1)"
      echo "$cmd $f exit=$code $valid ${stage:-stage=none} sha256=$digest"
    done
  done
  "$BIN" validate examples/does-not-exist.json >/dev/null; echo "validate examples/does-not-exist.json exit=$?"
  "$BIN" >/dev/null 2>&1; echo "no-arguments exit=$?"
  "$BIN" --no-such-flag >/dev/null 2>&1; echo "unknown-flag exit=$?"
} | tee "$OUT"
