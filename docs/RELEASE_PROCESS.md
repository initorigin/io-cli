# Release process — IO CLI

There is no registry. `publish = false` in `Cargo.toml`, and the GitHub Release — four
cross-compiled binaries plus `SHA256SUMS` — is the entire distribution channel. `install.sh`,
`install.ps1`, the Homebrew formula at `Formula/io.rb` and the scoop manifest at `bucket/io.json`
all read that one Release. So "released" here means the Release exists and its assets verify, not
that anything was uploaded to a package index.

The formula and the manifest are generated from a Release rather than written by hand, and
therefore updated **after** it is cut — step 10 below.

## Versioning

The product stays on `0.x` until its owner says otherwise. Within that:

- A new capability is a **minor** bump.
- Remediation, and a release whose subject is making the documentation true, is a **patch**.
- A breaking change is a minor bump, because `0.x` has no other room for one.

## The order

The sequence is not a preference. Each step exists because doing it later put something
irreversible in front of something reviewable.

1. **Branch.** `feat/<version>` cut from `develop`, whatever the diff touches.
2. **Work, committed as it lands.** Code, tests, the CHANGELOG entry, the README, the guide page
   and `docs/config.example.toml` in the same commit stream — not a documentation pass at the
   end. Bump `Cargo.toml`'s `version` and re-run the suite: nothing in the release tooling checks
   it, so a record sealed over the wrong version describes a tree that never shipped.
3. **Gates, last.** Run CI's exact invocations — including `cargo clippy --all-targets --
   -D warnings` with the flag, which is stronger than the same command without it. Anything that
   touches the tree after the gates invalidates them.
4. **Seal the release record**, before any merge: write it with `status: released`, run the
   `COMPLETING` then `RELEASED` transitions, append the record's SHA-256 to `.ultraship/releases.lock`
   by hand, and validate. The record states what was verified *before* shipping; sealing it
   afterwards makes it a description written with the answer already in hand.
5. **PR `feat/<version>` → `develop`.** Merge through the PR — a ruleset requires it.
6. **PR `develop` → `main`.** This PR *is* the release. Before opening it, check that `develop`'s
   **tree** matches the one the gates ran on — `git diff --quiet feat/<version> develop`, not a
   comparison of head SHAs, which a merge commit makes useless. A dependency bump merged above
   the feature commit would otherwise ship a tree the sealed record does not describe.
7. **Push the tag alone.** `git tag v<version> && git push <url> v<version>`. Never
   `gh release create` — `release.yml` triggers on `v*` and cuts the Release itself. **Verify the
   run exists**; a tag push can trigger nothing at all.
8. **Sync `develop` with `main`.** `git merge --ff-only main` on `develop`, then push. Every
   release, unconditionally, even when the two look equal.
9. **Smoke the published artifact.** Download it, check its checksum against the published
   `SHA256SUMS`, run it, and exercise the release's own headline surface — not just `--version`.
   0.30.1 exists because 0.30.0's smoke checked the version string and nothing else, over a
   subcommand that did not exist.
10. **Point the tap and the bucket at the new Release.** `scripts/update-tap.sh <version>` reads
    that Release's own `SHA256SUMS` and rewrites `Formula/io.rb` and `bucket/io.json`; open a pull
    request with the result and merge it. Until it lands, `brew install io` and `scoop install io`
    serve the previous version — the two files are a function of a **published** Release, so they
    cannot be updated in the release commit itself: the artifacts they name do not exist yet.

    **The workflow does not do this**, and cannot. `main` and `develop` both carry a
    `pull_request` ruleset whose only bypass actor is `OrganizationAdmin`, and GitHub Actions may
    not create pull requests here, so an automated tap update would need a granted ruleset bypass
    or a stored admin credential on a public repository. Neither is worth one merge click per
    release.

    `brew` reads a tap's **default branch**, which is `develop`, so the tap is live when that pull
    request merges and needs no `main` merge of its own.

## What the workflow builds

`release.yml` builds four targets — `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc` — runs `--version` on each where the host
can execute it, packages them, writes `SHA256SUMS`, and creates the Release. The release title is
the bare `io-cli X.Y.Z`, and the notes come from the CHANGELOG entry.

## Branch rules

`main` holds released versions only, and its **only** source is `develop`. Nothing else is ever
merged into it: a fix routes through `develop` like everything else. Branches are never deleted
after a merge.

## Attribution

Commits, PR titles and bodies, issue bodies, release notes and CHANGELOG entries carry no AI
attribution of any kind — no trailer, no generated-with line, no badge. They read as the author's
own work.
