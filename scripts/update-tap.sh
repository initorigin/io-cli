#!/bin/sh
# Point Formula/io.rb and bucket/io.json at a released version.
#
#   scripts/update-tap.sh 0.38.0
#
# Run it after the Release is cut, then open an ordinary pull request with the
# two files it changed. The release workflow cannot do this itself: `main` and
# `develop` both carry a `pull_request` ruleset whose only bypass actor is
# `OrganizationAdmin`, so a push from Actions is refused and Actions may not open
# the pull request either.
#
# It fetches that Release's SHA256SUMS, refuses anything it cannot read as four
# 64-character lowercase digests for the four artifacts the workflow builds, and
# only then rewrites the version and the checksums in those two files. The
# refusal is the point: a formula that names a digest for an artifact nobody
# uploaded installs nothing, and it fails on the user's machine rather than here.
#
# Every comparison is guarded by `[ -n "$want" ]` first. Two absent values
# compare equal, and this repository has printed a success line over exactly that
# — an empty digest matching an empty digest is not a match, it is a missing file.
#
# Running it twice over the same Release changes no byte the second time, so
# `git diff` after a run shows the release and nothing else.
#
# Environment:
#   IO_BASE_URL  fetch SHA256SUMS from here instead of the GitHub Release. Only
#                the fetch moves: the URLs written into the two files are always
#                the real Release ones, whatever this is set to.

set -eu

REPO="initorigin/io-cli"
DOWNLOADS="https://github.com/$REPO/releases/download"

MAC_ARM="aarch64-apple-darwin"
MAC_INTEL="x86_64-apple-darwin"
LINUX="x86_64-unknown-linux-musl"
WINDOWS="x86_64-pc-windows-msvc"

die() { printf 'update-tap: %s\n' "$*" >&2; exit 1; }

[ $# -eq 1 ] || die "usage: scripts/update-tap.sh <version>"
version="$1"
case "$version" in
    ""|*[!0-9.]*|.*|*.) die "'$version' is not a version number (expected e.g. 0.38.0)" ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
formula="$root/Formula/io.rb"
manifest="$root/bucket/io.json"
[ -f "$formula" ] || die "no such file: $formula"
[ -f "$manifest" ] || die "no such file: $manifest"

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
else
    die "this script needs curl or wget and neither is on PATH"
fi

base="${IO_BASE_URL:-$DOWNLOADS/v$version}"

work=$(mktemp -d 2>/dev/null || mktemp -d -t io-update-tap) ||
    die "could not make a temporary directory"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT INT TERM

sums="$work/SHA256SUMS"
fetch "$base/SHA256SUMS" "$sums" || die "could not download $base/SHA256SUMS"
grep -q '[^[:space:]]' "$sums" || die "$base/SHA256SUMS is empty; nothing was changed"

# The digest for one artifact, or nothing. `$2 == name` rather than a substring
# match, so a line for a different artifact cannot answer for this one.
digest_for() {
    awk -v name="$1" '$2 == name { print $1; exit }' "$sums"
}

# One artifact's digest, or a refusal naming the artifact. Absence and a
# malformed value are the same answer here — neither is something to write into a
# file an installer trusts.
want() {
    _archive="io-$version-$1.$2"
    _digest=$(digest_for "$_archive")
    [ -n "$_digest" ] || die "SHA256SUMS does not mention $_archive; nothing was changed"
    # The character set is written out rather than as `[!0-9a-f]`. A range in a
    # bracket expression is resolved by the locale's collating order, and in the
    # common UTF-8 locales `a-f` spans `aAbBcCdDeEfF` — so the range spelling
    # accepts an UPPERCASE digest, which is the one shape a case-sensitive
    # comparison against sha256sum's output would then never match.
    case "$_digest" in
        *[!0123456789abcdef]*) die "the digest for $_archive is not lowercase hex: $_digest" ;;
    esac
    [ ${#_digest} -eq 64 ] ||
        die "the digest for $_archive is ${#_digest} characters, not 64: $_digest"
    printf '%s\n' "$_digest"
}

mac_arm=$(want "$MAC_ARM" tar.gz)
mac_intel=$(want "$MAC_INTEL" tar.gz)
linux=$(want "$LINUX" tar.gz)
windows=$(want "$WINDOWS" zip)

# Nothing is written until all four have been read, so a Release missing one
# artifact leaves both files exactly as they were.
#
# Both rewrites replace the quoted VALUE on a line and leave the rest of the line
# alone, which is what keeps indentation, trailing commas and every untouched key
# byte-identical across runs.
rewrite() {
    _file="$1"
    shift
    awk "$@" "$_file" > "$work/out" || die "could not rewrite $_file"
    cat "$work/out" > "$_file"
}

rewrite "$formula" \
    -v v="$version" -v downloads="$DOWNLOADS" \
    -v arm="$MAC_ARM" -v intel="$MAC_INTEL" -v musl="$LINUX" \
    -v h_arm="$mac_arm" -v h_intel="$mac_intel" -v h_musl="$linux" '
    BEGIN { sha[arm] = h_arm; sha[intel] = h_intel; sha[musl] = h_musl }
    # No version rule: the formula declares no `version` stanza, because
    # Homebrew scans it out of the url and `brew audit --strict` refuses a
    # second declaration of it. Rewriting the urls below is therefore what
    # moves the formula to a new version.
    /^[[:space:]]*url "/ {
        target = ""
        if (index($0, arm)) target = arm
        else if (index($0, intel)) target = intel
        else if (index($0, musl)) target = musl
        if (target == "") {
            print "update-tap: this url names no known target: " $0 > "/dev/stderr"
            exit 1
        }
        sub(/"[^"]*"/,
            "\"" downloads "/v" v "/io-" v "-" target ".tar.gz\"")
        pending = target
        print
        next
    }
    /^[[:space:]]*sha256 "/ {
        if (pending == "") {
            print "update-tap: a sha256 with no url above it: " $0 > "/dev/stderr"
            exit 1
        }
        sub(/"[^"]*"/, "\"" sha[pending] "\"")
        pending = ""
        print
        next
    }
    { print }
'

# The autoupdate block is a template for the version AFTER this one, so its
# `$version` placeholders are left alone. A line carrying one is copied through
# untouched — substituting it would turn the template into a second copy of the
# concrete URL and scoop would stop being able to follow the next Release.
rewrite "$manifest" \
    -v v="$version" -v downloads="$DOWNLOADS" -v target="$WINDOWS" -v hash="$windows" '
    index($0, "$version") { print; next }
    /"version":/ { sub(/: "[^"]*"/, ": \"" v "\""); print; next }
    /"url": "/ {
        sub(/: "[^"]*"/,
            ": \"" downloads "/v" v "/io-" v "-" target ".zip\"")
        print
        next
    }
    /"hash": "/ { sub(/: "[^"]*"/, ": \"" hash "\""); print; next }
    /"extract_dir": "/ { sub(/: "[^"]*"/, ": \"io-" v "-" target "\""); print; next }
    { print }
'

printf '%s\n' "Formula/io.rb and bucket/io.json now name $version:"
printf '  %s  %s\n' "$mac_arm" "io-$version-$MAC_ARM.tar.gz"
printf '  %s  %s\n' "$mac_intel" "io-$version-$MAC_INTEL.tar.gz"
printf '  %s  %s\n' "$linux" "io-$version-$LINUX.tar.gz"
printf '  %s  %s\n' "$windows" "io-$version-$WINDOWS.zip"
