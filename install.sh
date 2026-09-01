#!/bin/sh
# Install the latest netlimit GitHub Release for this machine.
#
# Public repo (Raspberry Pi 64-bit or x86_64 Linux):
#   curl -sSfL https://raw.githubusercontent.com/virtuoz-afk/netlimit/main/install.sh | sh
#
# Private repo (after `gh auth login`, or with GITHUB_TOKEN set):
#   curl -sSfL https://raw.githubusercontent.com/virtuoz-afk/netlimit/main/install.sh | sh
#   NETLIMIT_REPO=asapdemo/netlimit sh install.sh
#
# Env:
#   NETLIMIT_REPO       owner/name (default: virtuoz-afk/netlimit)
#   NETLIMIT_VERSION    tag such as v0.2.0, or "latest" (default: latest)
#   INSTALL_DIR         destination directory (default: /usr/local/bin)
#   GITHUB_TOKEN / GH_TOKEN   required for private repos unless `gh` is logged in
#
# Raspberry Pi: 64-bit OS only (uname -m → aarch64). Then run: sudo netlimit

set -eu

REPO="${NETLIMIT_REPO:-virtuoz-afk/netlimit}"
VERSION="${NETLIMIT_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BIN_NAME="netlimit"

usage() {
  sed -n '2,18p' "$0" | sed 's/^# \?//'
  exit 0
}

case "${1:-}" in
  -h|--help) usage ;;
esac

err() {
  printf 'install.sh: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || err "missing command: $1"
}

github_token() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    printf '%s' "$GITHUB_TOKEN"
  elif [ -n "${GH_TOKEN:-}" ]; then
    printf '%s' "$GH_TOKEN"
  fi
}

detect_archive() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Linux) ;;
    *) err "unsupported OS '$os' (netlimit is Linux-only)" ;;
  esac
  case "$arch" in
    x86_64|amd64) printf 'netlimit-linux-x86_64' ;;
    aarch64|arm64) printf 'netlimit-linux-aarch64' ;;
    armv7l|armv6l|armhf)
      err "32-bit ARM ($arch) is not shipped. Use 64-bit Raspberry Pi OS (aarch64)."
      ;;
    *) err "unsupported architecture '$arch' (need x86_64 or aarch64)" ;;
  esac
}

asset_url() {
  archive="$1"
  file="$2"
  if [ "$VERSION" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/%s' "$REPO" "$file"
  else
    printf 'https://github.com/%s/releases/download/%s/%s' "$REPO" "$VERSION" "$file"
  fi
}

api_release_url() {
  if [ "$VERSION" = "latest" ]; then
    printf 'https://api.github.com/repos/%s/releases/latest' "$REPO"
  else
    printf 'https://api.github.com/repos/%s/releases/tags/%s' "$REPO" "$VERSION"
  fi
}

# Download $1 (URL) to $2 (path). Optional Authorization header via $3.
curl_get() {
  url="$1"
  dest="$2"
  token="${3:-}"
  if [ -n "$token" ]; then
    curl -fsSL --retry 3 \
      -H "Authorization: Bearer $token" \
      -H "Accept: application/octet-stream" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      -o "$dest" \
      "$url"
  else
    curl -fsSL --retry 3 -o "$dest" "$url"
  fi
}

download_public() {
  dir="$1"
  archive="$2"
  tarball="${archive}.tar.gz"
  sums="${tarball}.sha256"
  curl_get "$(asset_url "$archive" "$tarball")" "$dir/$tarball"
  curl_get "$(asset_url "$archive" "$sums")" "$dir/$sums"
}

download_gh() {
  dir="$1"
  archive="$2"
  tarball="${archive}.tar.gz"
  sums="${tarball}.sha256"
  if [ "$VERSION" = "latest" ]; then
    gh release download -R "$REPO" -p "$tarball" -p "$sums" -D "$dir"
  else
    gh release download "$VERSION" -R "$REPO" -p "$tarball" -p "$sums" -D "$dir"
  fi
}

# Private GitHub assets must be fetched via the API (browser URLs 404).
download_api() {
  dir="$1"
  archive="$2"
  token="$3"
  tarball="${archive}.tar.gz"
  sums="${tarball}.sha256"
  json="$dir/release.json"

  curl -fsSL --retry 3 \
    -H "Authorization: Bearer $token" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    -o "$json" \
    "$(api_release_url)"

  python3 - "$json" "$tarball" "$sums" "$dir" "$token" <<'PY'
import json, os, sys, urllib.request

json_path, tarball, sums, dest, token = sys.argv[1:6]
with open(json_path, encoding="utf-8") as fh:
    rel = json.load(fh)
if "message" in rel and "assets" not in rel:
    sys.stderr.write("GitHub API: %s\n" % rel["message"])
    sys.exit(1)

wanted = {tarball, sums}
found = {}
for asset in rel.get("assets", []):
    name = asset.get("name")
    if name in wanted:
        found[name] = asset["url"]

missing = wanted - set(found)
if missing:
    sys.stderr.write("release is missing assets: %s\n" % ", ".join(sorted(missing)))
    sys.exit(1)

headers = {
    "Authorization": "Bearer %s" % token,
    "Accept": "application/octet-stream",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "netlimit-install",
}
for name, url in found.items():
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp, open(os.path.join(dest, name), "wb") as out:
        out.write(resp.read())
PY
}

download_release() {
  dir="$1"
  archive="$2"
  token="$(github_token)"

  if download_public "$dir" "$archive" 2>/dev/null; then
    return 0
  fi

  rm -f "$dir/${archive}.tar.gz" "$dir/${archive}.tar.gz.sha256"

  printf 'Public download failed (repo may be private). Trying authenticated download…\n' >&2

  if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    download_gh "$dir" "$archive"
    return 0
  fi

  if [ -n "$token" ]; then
    need_cmd python3
    download_api "$dir" "$archive" "$token"
    return 0
  fi

  err "cannot download $archive from $REPO.

If the repository is private, either:
  • run  gh auth login   on this machine, or
  • export GITHUB_TOKEN (a PAT with repo scope), or
  • make the GitHub repo public so release assets are anonymously downloadable.

Override the repo with NETLIMIT_REPO=owner/name if needed."
}

verify_checksum() {
  dir="$1"
  archive="$2"
  need_cmd sha256sum
  (
    cd "$dir"
    sha256sum -c "${archive}.tar.gz.sha256"
  )
}

install_binary() {
  src="$1"
  dest="$INSTALL_DIR/$BIN_NAME"

  if [ ! -d "$INSTALL_DIR" ]; then
    if mkdir -p "$INSTALL_DIR" 2>/dev/null; then
      :
    else
      sudo mkdir -p "$INSTALL_DIR"
    fi
  fi

  if [ -w "$INSTALL_DIR" ]; then
    install -m 755 "$src" "$dest"
  else
    printf 'Writing %s requires sudo.\n' "$dest"
    sudo install -m 755 "$src" "$dest"
  fi
  printf 'Installed %s\n' "$dest"
}

need_cmd uname
need_cmd curl
need_cmd tar
need_cmd mktemp
need_cmd install

ARCHIVE="$(detect_archive)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT HUP

printf 'Downloading %s (%s) from %s…\n' "$BIN_NAME" "$ARCHIVE" "$REPO"
download_release "$TMP" "$ARCHIVE"
verify_checksum "$TMP" "$ARCHIVE"

tar -xzf "$TMP/${ARCHIVE}.tar.gz" -C "$TMP"
BIN="$TMP/$ARCHIVE/$BIN_NAME"
[ -f "$BIN" ] || err "archive did not contain $ARCHIVE/$BIN_NAME"

install_binary "$BIN"

if "$INSTALL_DIR/$BIN_NAME" --version >/dev/null 2>&1; then
  "$INSTALL_DIR/$BIN_NAME" --version
fi

printf '\nNext:\n  sudo apt install -y iproute2   # if not already installed\n  sudo netlimit\n'
