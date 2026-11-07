# The Homebrew formula for io. The repository is the tap, so it is added by URL:
#
#   brew tap initorigin/io-cli https://github.com/initorigin/io-cli
#   brew install initorigin/io-cli/io
#
# The formula ships the Release's own prebuilt artifacts and has no build step.
# io links io-harness, which carries rusqlite with a bundled C SQLite: building
# from source here would put a C toolchain and several minutes between
# `brew install` and a working binary, and it would hand the user bytes nobody
# published a checksum for. The archives named below are the ones the release
# workflow built for macOS and Linux, and every sha256 is copied out of that Release's
# SHA256SUMS by `scripts/update-tap.sh`. Never by hand — a hand-copied digest is
# how a formula comes to name an artifact that does not exist.
#
# The version the URLs name is the newest RELEASED one, which during a release is
# one behind the version being built: an artifact that has not been uploaded yet
# has no checksum to name. The generator runs after the Release is cut and its
# change goes in through an ordinary pull request, because the `pull_request`
# ruleset on `main` and `develop` bypasses for `OrganizationAdmin` alone and the
# release workflow is not one.
#
# **There is deliberately no `version` stanza.** Homebrew scans the version out
# of the url, `brew audit --strict` calls an explicit one redundant, and it is
# worse than redundant: it is a second declaration of a fact the urls already
# carry, and the two can disagree. A formula whose `version` said 0.37.0 over
# urls naming 0.36.0 would install 0.36.0 and report the other number from its
# own `test` block.
class Io < Formula
  desc "Terminal agent that shows what it may do, what it spends and what it refused"
  homepage "https://github.com/initorigin/io-cli"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/initorigin/io-cli/releases/download/v0.42.0/io-0.42.0-aarch64-apple-darwin.tar.gz"
      sha256 "0ad01bb5dd8909774798f4e5417efbe0d644c19b89eb0e2b65ae445197718c25"
    end

    if Hardware::CPU.intel?
      url "https://github.com/initorigin/io-cli/releases/download/v0.42.0/io-0.42.0-x86_64-apple-darwin.tar.gz"
      sha256 "a004d6dc39b238ca8ae6526b01186689ff1867a095d67d1516d6ec1e3f3fa392"
    end
  end

  # Linux is musl only, exactly as the release matrix is: one static binary per
  # architecture. The arm64 branch first exists in the Release after 0.41.0, and
  # `scripts/update-tap.sh` writes it the first time it runs over a Release that
  # carries that artifact — until then this machine gets Homebrew's own refusal
  # rather than a URL that 404s halfway through an install.
  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/initorigin/io-cli/releases/download/v0.42.0/io-0.42.0-aarch64-unknown-linux-musl.tar.gz"
      sha256 "82e3c2986bce5de2f4fc99d14c8818880d110b775195d4106fb1f69fd903aa83"
    end

    if Hardware::CPU.intel?
      url "https://github.com/initorigin/io-cli/releases/download/v0.42.0/io-0.42.0-x86_64-unknown-linux-musl.tar.gz"
      sha256 "2d901a2d08ca5e45d17a42dadb2e19968a6d07bd4a7a9e0ec8ce32f29168fa58"
    end
  end

  # Each archive holds one versioned directory — `io-<version>-<target>/` with the
  # binary beside LICENSE, NOTICE, README.md and CHANGELOG.md — and Homebrew
  # descends into that single directory before this runs. Only the binary is
  # installed; the licence and the notice travel with the Release itself.
  def install
    bin.install "io"
  end

  # The binary's own words, not the formula's: proof that what was unpacked runs
  # here and is the version this file claims to have installed.
  test do
    assert_match version.to_s, shell_output("#{bin}/io --version")
  end
end
