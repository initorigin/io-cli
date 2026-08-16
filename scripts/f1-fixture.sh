#!/bin/sh
# Set up a clean fixture for F1, the release's one manual criterion.
#
# It makes a throwaway workspace with a failing test in it, points the
# configuration at a throwaway directory so your own ~/.config/io is untouched,
# and prints the steps. It does NOT export a provider key: the wizard's masked
# credential field is part of what F1 exercises, so the key gets pasted.
#
#   sh scripts/f1-fixture.sh
#
# Nothing here is part of the shipped product; it exists so the run is one paste
# rather than a setup.

set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
binary="$repo/target/release/io"

if [ ! -x "$binary" ]; then
    echo "build it first: cargo build --release" >&2
    exit 1
fi

work=$(mktemp -d)
config=$(mktemp -d)

mkdir -p "$work/src"
cat > "$work/src/lib.rs" <<'EOF'
pub fn add(a: u32, b: u32) -> u32 {
    // Deliberately wrong, so there is something real to fix.
    a - b
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_adds() {
        assert_eq!(super::add(2, 3), 5);
    }
}
EOF
cat > "$work/Cargo.toml" <<'EOF'
[package]
name = "f1-fixture"
version = "0.0.0"
edition = "2021"
EOF

cat <<EOF

  F1 — the live first run
  =======================

  Workspace     $work
  Config home   $config   (throwaway; your own configuration is untouched)
  Binary        $binary

  1. Start recording. Cmd+Shift+5 on macOS records the screen; the recording IS
     the release evidence and the README's screenshot.

  2. Then paste this whole block into a NEW terminal window:

       export IO_CONFIG_HOME="$config"
       unset IO_CONFIG OPENROUTER_API_KEY
       cd "$work"
       "$binary"

  3. The wizard should walk you through, in this order:

       welcome      Enter
       provider     OpenRouter is the first row       Enter
       credential   PASTE your key. It must show bullets, never characters.
                    Enter — it is verified against the live endpoint here, so a
                    bad key comes back to this screen with OpenRouter's own words
       model        pick one                          Enter
       theme        arrow up and down — the sample transcript below the picker
                    should re-render in each theme as you move                 Enter
       posture      "Sandboxed workspace" is the first row                     Enter
       confirm      it names the exact path and says the key is written 0600.
                    Nothing is on disk until you press Enter here.

  4. Type a task and watch it stream:

       make the failing test in src/lib.rs pass, then tell me what you changed

  5. Type a second task, and press Ctrl+C while it is still working. The turn
     should stop, the partial answer should stay in the scrollback, and the
     prompt should come back ready.

  6. Press Ctrl+D on an empty prompt to exit.

  7. NOW, in the same terminal, with io no longer running — this is the half no
     test can prove:

       - Cmd+F and search for something the model said. It should be found.
       - Drag-select a passage with the mouse and copy it. It should copy.
       - Scroll up. The whole conversation should still be there.
       - Your shell should echo normally.

  8. Check the file it wrote, and its mode:

       ls -l "$config/io.toml"        # expect -rw-------
       cat "$config/io.toml"

  When you are done, tell me what you saw — especially anything that looked
  wrong, ugly or confusing. The look is a human gate on this release and it is
  not something a test can approve.

EOF
