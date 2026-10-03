#!/usr/bin/env bash
set -euo pipefail

prefix="${HOME:-}/.local"
while (($#)); do
  case "$1" in
    --prefix)
      if (($# < 2)) || [[ -z "$2" || "$2" == -* ]]; then
        printf '%s\n' 'Error: --prefix requires a directory.' >&2
        exit 2
      fi
      prefix="$2"
      shift 2
      ;;
    -h|--help)
      printf '%s\n' 'Usage: scripts/install-source.sh [--prefix DIRECTORY]' \
        'Build the locked release binary and install it into DIRECTORY/bin.' \
        'Default prefix: $HOME/.local. Requires Rust and Cargo. Never invokes sudo.'
      exit 0
      ;;
    *)
      printf 'Error: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ "$prefix" != /* ]]; then
  prefix="$PWD/$prefix"
fi
for command in cargo rustc install; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Error: required command not found: %s\n' "$command" >&2
    exit 1
  fi
done
if [[ -z "${HOME:-}" && "$prefix" == /.local ]]; then
  printf '%s\n' 'Error: HOME is unset. Supply --prefix DIRECTORY.' >&2
  exit 2
fi

source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
host="$(rustc -vV | sed -n 's/^host: //p')"
if [[ -z "$host" ]]; then
  printf '%s\n' 'Error: cannot determine the Rust host target.' >&2
  exit 1
fi
cargo build --manifest-path "$source_dir/Cargo.toml" --locked --release \
  --bin baseballhour --target "$host" --target-dir "$source_dir/target" \
  --jobs "${CARGO_BUILD_JOBS:-2}"
install -d "$prefix/bin"
install -m 755 "$source_dir/target/$host/release/baseballhour" "$prefix/bin/baseballhour"
printf 'Installed %s\n' "$prefix/bin/baseballhour"
printf 'Add %s to PATH if needed.\n' "$prefix/bin"
