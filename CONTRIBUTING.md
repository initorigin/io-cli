# Contributing

Thank you for taking the time.

## The branch flow

`develop` is the working branch and `main` is the release branch. Work happens on
a branch cut from `develop` and reaches it through a pull request. `main` takes
nothing but a release merge from `develop`, also through a pull request. Nothing
is pushed directly to either.

## Before you open a pull request

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three on macOS, Linux and Windows. Windows is not optional: it
is where a terminal renderer's assumptions break.

## What belongs here, and what does not

io-cli contains no agent loop, no provider client, no tool implementation, no
sandbox, no policy engine and no session store. All of that is
[io-harness](https://github.com/initorigin/io-harness). If a change needs one of
them, it is a change to the harness and this repository consumes it. A test in
`tests/dependencies.rs` enforces this by inspecting the dependency set, so a pull
request that adds an HTTP client, a TLS stack, a database crate or a sandboxing
crate fails rather than being argued about in review.

Two more properties are asserted structurally and are not up for negotiation in a
patch:

- The alternate screen is never entered and the mouse is never captured.
- No test asserts on wall-clock time. Measurements are recorded, never gated.

## Licence

By contributing you agree that your contribution is licensed under Apache-2.0,
the licence this project carries.
