#!/bin/sh
set -eu

fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
usage() {
  printf '%s\n' 'Usage: install.sh [--version VERSION] [--prefix DIRECTORY]' \
    'Download and verify a native Baseball Hour release. No Rust or sudo required.' \
    'Defaults: latest release, $HOME/.local/bin/baseballhour.'
}

prefix=${HOME:+$HOME/.local}
version=latest
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version|--prefix)
      option=$1
      [ "$#" -ge 2 ] && [ -n "$2" ] || fail "$option requires a value."
      case "$2" in --*) fail "$option requires a value." ;; esac
      if [ "$option" = --version ]; then version=$2; else prefix=$2; fi
      shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "Unknown option: $1. Use --help." ;;
  esac
done
[ -n "$prefix" ] || fail 'HOME is unset. Supply --prefix DIRECTORY.'
case "$prefix" in /*) ;; *) prefix=$PWD/$prefix ;; esac

for tool in curl tar mktemp install awk; do
  command -v "$tool" >/dev/null 2>&1 || fail "Missing $tool. Install it and retry."
done
if command -v sha256sum >/dev/null 2>&1; then
  sha_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  sha_tool=shasum
else
  fail 'Install sha256sum or shasum to verify the download.'
fi

case "$(uname -s)" in
  Darwin) platform=apple-darwin ;;
  Linux) platform=unknown-linux-musl ;;
  *) fail 'Supported platforms are macOS and Linux. See the source installation guide.' ;;
esac
case "$(uname -m)" in
  x86_64|amd64) architecture=x86_64 ;;
  aarch64|arm64) architecture=aarch64 ;;
  *) fail 'Supported processors are x86_64 and ARM64. See the source installation guide.' ;;
esac

fetch() {
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --connect-timeout 10 --max-time 120 --retry 2 "$@"
}

releases=https://github.com/justinramos101/baseballhour/releases
if [ "$version" = latest ]; then
  resolved=$(fetch --output /dev/null --write-out '%{url_effective}' "$releases/latest") \
    || fail 'Could not find the latest release. Check your connection or use --version.'
  version=${resolved##*/}
fi
case "$version" in v*) ;; *) version=v$version ;; esac
printf '%s\n' "$version" | LC_ALL=C awk '
  /^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/ { valid=1 }
  END { exit !valid }
' || fail 'Use a release version such as v0.1.0.'

name=baseballhour-$version-$architecture-$platform
work=$(mktemp -d)
staged=
cleanup() {
  rm -rf "$work"
  [ -z "$staged" ] || rm -f "$staged"
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM
printf 'Downloading Baseball Hour %s for %s-%s...\n' "$version" "$architecture" "$platform"
fetch --output "$work/archive.tar.gz" "$releases/download/$version/$name.tar.gz" \
  || fail 'Could not download the release. Check the version and your connection.'
fetch --output "$work/checksum" "$releases/download/$version/$name.tar.gz.sha256" \
  || fail 'Could not download the checksum. Nothing was installed.'
expected=$(awk 'NR == 1 { print $1 }' "$work/checksum")
printf '%s\n' "$expected" | LC_ALL=C awk '
  length($0) == 64 && $0 !~ /[^0-9a-f]/ { valid=1 }
  END { exit !valid }
' || fail 'The release checksum is invalid. Nothing was installed.'
if [ "$sha_tool" = sha256sum ]; then
  actual=$(sha256sum "$work/archive.tar.gz" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$work/archive.tar.gz" | awk '{print $1}')
fi
[ "$actual" = "$expected" ] || fail 'Checksum mismatch. Nothing was installed. Download again.'
tar -xzf "$work/archive.tar.gz" -C "$work" "$name/baseballhour" \
  || fail 'The release archive is invalid. Nothing was installed.'
binary=$work/$name/baseballhour
[ -f "$binary" ] && [ ! -L "$binary" ] || fail 'The release contains no regular executable.'
mkdir -p "$prefix/bin"
[ ! -d "$prefix/bin/baseballhour" ] || fail 'The installation path is a directory.'
staged=$(mktemp "$prefix/bin/.baseballhour.XXXXXX")
install -m 755 "$binary" "$staged"
installed_version=$("$staged" --version) || fail 'This binary could not run on your system. Nothing was replaced.'
[ "$installed_version" = "baseballhour ${version#v}" ] || fail 'The binary version does not match the release.'
mv -f "$staged" "$prefix/bin/baseballhour"
staged=
printf 'Installed %s\n' "$prefix/bin/baseballhour"
printf 'Run "%s/bin/baseballhour" --demo for an offline tour.\n' "$prefix"
case ":${PATH:-}:" in
  *":$prefix/bin:"*) ;;
  *) printf 'Add %s/bin to PATH to run baseballhour from anywhere.\n' "$prefix" ;;
esac
