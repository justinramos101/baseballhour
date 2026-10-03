#!/bin/sh
set -eu

launcher=$0
case "$launcher" in
  */*) ;;
  *) launcher=$(command -v "$launcher") ;;
esac
links=0
while [ -L "$launcher" ]; do
  links=$((links + 1))
  if [ "$links" -gt 40 ]; then
    echo "baseballhour: too many launcher symlinks" >&2
    exit 1
  fi
  directory=$(CDPATH= cd -P "$(dirname "$launcher")" && pwd)
  target=$(readlink "$launcher")
  case "$target" in
    /*) launcher=$target ;;
    *) launcher=$directory/$target ;;
  esac
done
package_dir=$(CDPATH= cd -P "$(dirname "$launcher")/.." && pwd)
os=$(uname -s)
arch=$(uname -m)
case "$os:$arch" in
@PLATFORMS@
  *)
    echo "baseballhour: unsupported platform $os/$arch; use macOS or Linux on x64 or ARM64" >&2
    exit 1
    ;;
esac
binary=$package_dir/$native
if [ ! -x "$binary" ]; then
  echo "baseballhour: packaged binary is missing or not executable: $binary" >&2
  exit 126
fi
exec "$binary" "$@"
