#!/bin/sh
# Install io on macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh
#
# What it does, in order: work out which artifact this machine needs, download it
# and the SHA256SUMS beside it, VERIFY THE ARTIFACT BEFORE UNPACKING IT, and move
# the binary into a directory the current user owns. No sudo, nothing written
# outside the user's own directories, and nothing left behind if anything fails.
#
# The checksum defends against a truncated download and a tampered asset. It does
# not defend against a compromised repository — piping a script from the internet
# into a shell is a trust-the-publisher model however the script is written, and
# the README says so rather than implying more.
#
# Environment:
#   IO_VERSION      install this version instead of the latest (e.g. 0.1.0)
#   IO_INSTALL_DIR  install here instead of ~/.local/bin
#   IO_BASE_URL     download from here instead of the GitHub Release

set -eu

REPO="initorigin/io-cli"
BIN="io"

say() { printf '%s\n' "$*"; }
die() { printf 'io install: %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "this script needs $1 and it is not on PATH"
}

need uname
need mkdir
need mv
need rm

# curl or wget, whichever is here.
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
    fetch_effective_url() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
    fetch_effective_url() { wget -qS --max-redirect=10 -O /dev/null "$1" 2>&1 | sed -n 's/^ *Location: *//p' | tail -1; }
else
    die "this script needs curl or wget and neither is on PATH"
fi

# sha256sum on Linux, shasum on macOS. Refusing to continue without one is the
# point: an install that skipped verification because the tool was missing would
# be the one case where this script does the thing it exists to prevent.
if command -v sha256sum >/dev/null 2>&1; then
    checksum() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
    checksum() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
    die "this script needs sha256sum or shasum to verify the download, and neither is on PATH"
fi

os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
    Darwin-arm64)   target="aarch64-apple-darwin" ;;
    Darwin-x86_64)  target="x86_64-apple-darwin" ;;
    Linux-x86_64)   target="x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64)
        die "there is no Linux arm64 build yet. Build from source: cargo build --release" ;;
    *)
        die "no build for $os $arch. Build from source: cargo build --release" ;;
esac

version="${IO_VERSION:-}"
if [ -z "$version" ]; then
    # The Release page redirects to the newest tag, so the newest version is
    # readable without an API token and without a JSON parser.
    latest=$(fetch_effective_url "https://github.com/$REPO/releases/latest") ||
        die "could not reach GitHub to find the latest version"
    version=${latest##*/tag/v}
    case "$version" in
        ""|*/*) die "could not work out the latest version from '$latest'" ;;
    esac
fi

base="${IO_BASE_URL:-https://github.com/$REPO/releases/download/v$version}"
stage="$BIN-$version-$target"
archive="$stage.tar.gz"
dest="${IO_INSTALL_DIR:-$HOME/.local/bin}"

# Everything happens in here until it has been verified, so a failure at any
# point leaves the target directory exactly as it was.
work=$(mktemp -d 2>/dev/null || mktemp -d -t io-install) || die "could not make a temporary directory"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT INT TERM

say "io $version for $target"

fetch "$base/$archive" "$work/$archive" || die "could not download $base/$archive"
fetch "$base/SHA256SUMS" "$work/SHA256SUMS" || die "could not download $base/SHA256SUMS"

expected=$(grep " $archive\$" "$work/SHA256SUMS" | cut -d' ' -f1 | head -1)
[ -n "$expected" ] || die "SHA256SUMS does not mention $archive"

actual=$(checksum "$work/$archive")
if [ "$expected" != "$actual" ]; then
    die "checksum mismatch for $archive
  expected $expected
  actual   $actual
Nothing was installed."
fi
say "checksum ok"

( cd "$work" && tar xzf "$archive" ) || die "could not unpack $archive"
[ -f "$work/$stage/$BIN" ] || die "$archive does not contain $BIN"

mkdir -p "$dest" || die "could not create $dest"
mv "$work/$stage/$BIN" "$dest/$BIN" || die "could not install into $dest"
chmod +x "$dest/$BIN"

say "installed $dest/$BIN"

# The PATH line is printed rather than written. Editing somebody's shell profile
# behind their back is the thing install scripts are disliked for, and the line
# is one they can read before they run it.
case ":$PATH:" in
    *":$dest:"*)
        say "run: $BIN"
        ;;
    *)
        say ""
        say "$dest is not on your PATH. Add this to your shell profile:"
        say ""
        say "    export PATH=\"$dest:\$PATH\""
        say ""
        say "then open a new shell and run: $BIN"
        ;;
esac
