//! F1, F6 and F7 — the boundary the operator chooses, the one the agent gets,
//! and the one act that never reaches the operator at all.
//!
//! F6 and F7 are deliberately not the same criterion. F6 is what the status line
//! says; F7 is what the next turn actually runs under. A mode indicator that is
//! not backed by the policy it names is invisible to every assertion on a
//! rendered line, which is why the second criterion asserts on an
//! `io_harness::Policy` and never on a word.
//!
//! F1 is the same question asked of one act. `io_harness::tools::git` refuses a
//! spawn on any effect that is not `Allow` without consulting an approver, so
//! there is no rendered sentence to assert on even in principle — the only place
//! the repair is visible is the verdict.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::{Act, Defaults, Effect, Policy};

use io_cli::app::App;
use io_cli::approval;
use io_cli::settings::Posture;
use io_cli::theme::DARK;

fn shift_tab() -> KeyEvent {
    // What a terminal without the Kitty keyboard protocol sends. It is the
    // spelling most terminals in use produce, and it carries no modifier at all.
    KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)
}

fn shifted_tab() -> KeyEvent {
    // What a terminal that has negotiated the Kitty protocol sends instead. Same
    // key on the same keyboard; a product that binds one spelling works on the
    // developer's terminal and silently does nothing on somebody else's.
    KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)
}

fn line(app: &App) -> String {
    app.status.line(120, &DARK).to_string()
}

/// **F6.** The key moves the posture, and the line says which one is in force.
#[test]
fn f6_shift_tab_cycles_the_posture_and_the_status_line_follows() {
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(Posture::Workspace));
    assert!(
        line(&app).contains("policy:workspace"),
        "the line must name the posture in force: {:?}",
        line(&app),
    );

    app.key(shift_tab());
    assert_eq!(app.posture(), Some(Posture::AskWrites));
    assert!(
        line(&app).contains("policy:ask-writes"),
        "the line did not follow the key: {:?}",
        line(&app),
    );

    app.key(shift_tab());
    assert_eq!(app.posture(), Some(Posture::ReadOnly));
    app.key(shift_tab());
    assert_eq!(
        app.posture(),
        Some(Posture::Workspace),
        "the cycle wraps — it is a cycle, not a list",
    );
}

/// **F6, the half that decides whether the key exists at all.** Both spellings
/// reach the same action.
#[test]
fn f6_both_spellings_of_shift_tab_are_the_same_key() {
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(Posture::Workspace));
    app.key(shifted_tab());
    assert_eq!(
        app.posture(),
        Some(Posture::AskWrites),
        "a terminal speaking the Kitty protocol sends Tab with a shift modifier",
    );

    app.key(shift_tab());
    assert_eq!(
        app.posture(),
        Some(Posture::ReadOnly),
        "a terminal without it sends BackTab, and it is the same key on the keyboard",
    );
}

/// A configuration file may hold a policy that is none of the three postures. The
/// line says so rather than naming one it is not, and the first press moves to a
/// posture the operator did choose.
#[test]
fn a_policy_that_is_not_one_of_the_three_says_so() {
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(None);
    assert!(
        line(&app).contains("policy:custom"),
        "an unrecognised policy must not be labelled as one of the three: {:?}",
        line(&app),
    );
    app.key(shift_tab());
    assert_eq!(app.posture(), Some(Posture::Workspace));
}

/// **F7.** Asserted on the policy, and as a verdict rather than as a field: the
/// question is what the agent may do, not what the struct says.
#[test]
fn f7_the_cycled_posture_is_what_the_next_turn_runs_under() {
    let base = Policy::default();
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(Posture::Workspace));

    let policy = approval::session_policy(&base, app.posture(), app.remembered(), false);
    assert_eq!(
        policy.check(Act::Write, "src/main.rs").effect,
        Effect::Allow,
        "the workspace posture writes inside the workspace without asking",
    );

    app.key(shift_tab());
    let policy = approval::session_policy(&base, app.posture(), app.remembered(), false);
    assert_eq!(
        policy.check(Act::Write, "src/main.rs").effect,
        Effect::Ask,
        "ask-before-writes has to actually ask",
    );

    app.key(shift_tab());
    let policy = approval::session_policy(&base, app.posture(), app.remembered(), false);
    assert_eq!(
        policy.check(Act::Write, "src/main.rs").effect,
        Effect::Deny,
        "read-only has to actually refuse",
    );
}

/// Cycling never unlocks what the file's own layers denied. The posture is the
/// tier default; a layer that denies a secret is not a default and is not moved
/// by a keystroke.
#[test]
fn no_posture_can_unlock_what_a_layer_denied() {
    let base = Policy::default();
    for posture in Posture::ALL {
        let policy = approval::session_policy(&base, Some(*posture), &[], false);
        assert_eq!(
            policy.check(Act::Write, ".env").effect,
            Effect::Deny,
            "{:?} unlocked a target the secrets layer denies",
            posture,
        );
    }
}

/// With no posture chosen the policy is the file's own, untouched. A release that
/// rebuilt it regardless would be one where a mapping bug is invisible until
/// somebody presses a key.
#[test]
fn no_posture_means_the_file_decides() {
    let base = Policy::default();
    assert_eq!(approval::session_policy(&base, None, &[], false), base);
}

/// The three postures are the three `io_harness::Defaults` sets `settings.rs`
/// already declares, and the mapping back is exact. A posture recognised as one it
/// is not would put a true-looking word beside a boundary it does not describe.
#[test]
fn a_posture_is_recognised_from_the_defaults_it_is() {
    for posture in Posture::ALL {
        assert_eq!(Posture::of(&posture.defaults()), Some(*posture));
    }
    assert_eq!(
        Posture::of(&Defaults {
            read: Effect::Deny,
            write: Effect::Deny,
            exec: Effect::Deny,
            net: Effect::Deny,
        }),
        None,
        "a policy nobody offered is not silently reported as one that was",
    );
}

/// **F1, rewritten by io-harness 0.70.0 and kept because the fact still matters.**
///
/// Through 0.69.0 this asserted that the recommended posture *refused* git, and
/// refused it where no approver was reachable: `Git::run` treated every effect
/// short of `Allow` as a hard refusal, which io-cli 0.25.0 reported as
/// io-harness#214. **0.70.0 closed that**, at all four sites carrying the
/// comparison, so an asking posture now raises an ordinary approval.
///
/// What is asserted now is the thing that did not change: the recommended posture
/// does not hand git out. It **asks**, which is a different answer from both
/// `Allow` and `Deny` and is the whole of what the fix bought — so `== Ask` is the
/// assertion, where the old one was `!= Allow`. That inequality was written
/// because it was "the whole predicate upstream applies"; upstream applies a
/// three-way match now, and an assertion that still read `!= Allow` would pass
/// just as happily if the effect became `Deny`.
///
/// The live arm is what proves the other half — that the approval is really
/// raised and the spawn really happens — because only a real run has an approver
/// in it.
#[test]
fn f1_an_asking_posture_asks_about_git_rather_than_allowing_it() {
    let base = Policy::default();
    let policy = approval::session_policy(&base, Some(Posture::AskWrites), &[], false);
    assert_eq!(
        policy.check(Act::Exec, "git").effect,
        Effect::Ask,
        "the recommended posture asks about git: not allowed outright, and — since \
         io-harness 0.70.0 — not refused with nobody consulted either",
    );
}

/// **F1.** The repair, offered through the mechanism that already exists for
/// everything else the operator allows for the session.
#[test]
fn f1_the_git_allowance_turns_that_refusal_into_an_allow() {
    let base = approval::session_policy(&Policy::default(), Some(Posture::AskWrites), &[], false);
    let policy = approval::effective_policy(&base, &[approval::git_allowance()]);
    assert_eq!(
        policy.check(Act::Exec, "git").effect,
        Effect::Allow,
        "with the allowance in force the seven git tools may spawn",
    );
}

/// **F1, and the half the sabotage attacks.** One rule, one program. An
/// `Act::Exec` pattern names a binary, so a grant written wider than the binary it
/// was for is a grant of the whole PATH.
#[test]
fn f1_the_git_allowance_changes_nothing_for_another_program() {
    let base = approval::session_policy(&Policy::default(), Some(Posture::AskWrites), &[], false);
    let before = base.check(Act::Exec, "curl").effect;
    let policy = approval::effective_policy(&base, &[approval::git_allowance()]);
    assert_eq!(
        policy.check(Act::Exec, "curl").effect,
        before,
        "allowing git must leave every other binary exactly where it was",
    );
    assert_ne!(
        policy.check(Act::Exec, "curl").effect,
        Effect::Allow,
        "an allowance for one program is not an allowance for the machine",
    );
}

/// **F1.** A layer that denies exec still denies it afterwards. The allowance
/// rides the same `remembered` layer as everything else, and a later layer may add
/// capability but may never re-allow what an earlier one denied — so this is a
/// property of the mechanism, asserted rather than assumed.
#[test]
fn f1_a_denied_exec_is_still_denied_after_the_git_allowance() {
    let base = Policy::default().layer("locked-down").deny_exec("git");
    for posture in Posture::ALL {
        let policy =
            approval::session_policy(&base, Some(*posture), &[approval::git_allowance()], false);
        assert_eq!(
            policy.check(Act::Exec, "git").effect,
            Effect::Deny,
            "{:?}: the allowance re-opened a spawn a layer had denied",
            posture,
        );
    }
}

/// **F8 — full access is wide, and the five things that keep it safe.**
///
/// It is the only surface in io that can make a machine's whole filesystem
/// writable by a model, and the contract asks for each mitigation to be defeated
/// separately. Four of the five are asserted here; the fifth — that it is never
/// written to a file — is a property of there being no writer, and
/// `tests/docs.rs` holds the sentence that says so.
///
/// Sabotage: add a fourth `Posture` variant and the first assertion fails; drop
/// the `full_access` branch in `App::set_posture` and the marker one does.
#[test]
fn f8_full_access_is_not_a_posture_and_is_never_one_keypress_away() {
    // **One: it is not in the cycle, because `Posture::ALL` does not contain it.**
    // A fourth variant would have put the widest grant in the product one key from
    // `read-only` by construction rather than by anybody deciding to.
    assert_eq!(
        Posture::ALL.len(),
        3,
        "full access became a posture, so `Shift+Tab` now reaches it",
    );
    assert!(
        !Posture::ALL
            .iter()
            .any(|posture| posture.short() == Posture::FULL_ACCESS),
        "the cycle names full access",
    );

    // **Two: pressing the key four times from `workspace` is back at `workspace`,
    // having drawn nothing else.** Asserted by walking the cycle rather than by
    // reading `ALL`, so a cycle that disagreed with its own list would fail here.
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(Posture::Workspace));
    // **Six presses, not four.** The contract's own criterion said four, which is
    // not the identity of a three-cycle — four presses land on `ask-writes`. Two
    // full turns of the cycle is the property that was meant, and it visits every
    // posture twice, so a fourth one appearing anywhere in the walk is caught.
    let mut seen = Vec::new();
    for _ in 0..6 {
        app.key(shift_tab());
        seen.push(app.status.policy.clone().unwrap_or_default());
    }
    assert!(
        !seen.iter().any(|word| word == Posture::FULL_ACCESS),
        "the posture cycle drew full access: {seen:?}",
    );
    assert_eq!(
        seen.len(),
        6,
        "the walk must actually have pressed the key six times",
    );
    assert_eq!(
        app.posture(),
        Some(Posture::Workspace),
        "two full turns of a three-cycle is back where it started",
    );

    // **Three: while it is in force every frame says so.** An unconfined session
    // that looks ordinary is worse than the grant itself, and the posture word
    // would otherwise read `custom` — true of the struct and useless to a reader.
    let mut unconfined = App::new(DARK, "opus-5");
    unconfined.set_posture(Posture::of(&approval::UNCONFINED));
    assert_eq!(
        Posture::of(&approval::UNCONFINED),
        None,
        "full access is deliberately not recognised as one of the three",
    );
    unconfined.set_full_access(true);
    assert!(
        line(&unconfined).contains(Posture::FULL_ACCESS),
        "a frame drawn under full access does not say so: {:?}",
        line(&unconfined),
    );

    // And a session that gave it back does not carry the marker.
    unconfined.set_full_access(false);
    assert!(
        !line(&unconfined).contains(Posture::FULL_ACCESS),
        "the marker survived the grant being taken back: {:?}",
        line(&unconfined),
    );

    // **Four: it replaces the tier defaults and cannot unlock a layer.** The
    // widest grant io offers still does not defeat a rule the operator wrote down.
    let policy = Policy {
        defaults: approval::UNCONFINED,
        ..Policy::default()
    };
    assert_eq!(
        policy.check(Act::Net, "example.com").effect,
        Effect::Allow,
        "full access did not widen the tier default it exists to widen",
    );
    assert_eq!(
        policy.check(Act::Read, ".env").effect,
        Effect::Deny,
        "full access unlocked a secret the built-in layer denies, which no posture \
         and no flag in this product may do",
    );
}

/// **F7 — escalation moves a fallback, and a written rule is untouched.**
///
/// This is the release's security argument and the pair is the whole test. A
/// refusal that came from `policy.defaults` is io-harness answering because no
/// rule matched, and turning that into a question is what stops a dead end. A
/// refusal that came from a `[[policy.layers]]` rule is a decision the operator
/// wrote down, and it must refuse exactly as it did before — silently, always.
///
/// **The negative arm is the one that matters.** An implementation that read
/// `Policy::check`'s `Effect` and turned every `Deny` into an approval would pass
/// every positive assertion here and ship the opposite of the claim. It is ruled
/// out structurally rather than caught: `approval::asking` takes a `Defaults` and
/// cannot see a layer. This asserts the structure holds.
///
/// Sabotage: make `asking` answer `Effect::Allow` instead of `Effect::Ask` and the
/// third assertion fails; apply it to the whole `Policy` rather than to its
/// defaults and the `.env` arm does.
#[test]
fn f7_escalation_moves_a_default_and_never_a_written_rule() {
    // `Policy::default()` denies the secret paths as RULES in a layer and denies
    // net as a tier DEFAULT. One policy carrying both kinds, which is what makes
    // the comparison honest rather than two fixtures chosen to agree.
    let base = Policy::default();
    assert_eq!(
        base.check(Act::Read, ".env").effect,
        Effect::Deny,
        "the fixture must start with a written deny or the negative arm proves nothing",
    );
    assert!(
        base.check(Act::Read, ".env").layer.is_some(),
        "`.env` must be refused by a LAYER for this test to be about the distinction it claims",
    );
    assert!(
        base.check(Act::Net, "example.com").layer.is_none(),
        "net must be refused by the tier DEFAULT, which is the thing escalation moves",
    );

    let off = approval::session_policy(&base, None, &[], false);
    let on = approval::session_policy(&base, None, &[], true);

    assert_eq!(
        off, base,
        "escalation off changed a policy nobody asked it to"
    );

    assert_eq!(
        on.check(Act::Net, "example.com").effect,
        Effect::Ask,
        "a deny that came from the tier default did not become a question, so the \
         dead end this release exists to remove is still there",
    );

    // **And the written rule is untouched.** Not `Ask` — a question here would be
    // io asking whether to ignore something the operator wrote down.
    assert_eq!(
        on.check(Act::Read, ".env").effect,
        Effect::Deny,
        "a `[[policy.layers]]` deny became askable, which is the release shipping \
         the opposite of its own security claim behind a green suite",
    );
    assert_eq!(
        on.check(Act::Write, "id_rsa").effect,
        Effect::Deny,
        "the same, for a write to a private key",
    );

    // **An allow stays an allow.** Escalation only moves the strictest tier one
    // step towards a question; it never tightens anything either.
    let permissive = Policy::permissive();
    let widened = approval::session_policy(&permissive, None, &[], true);
    assert_eq!(
        widened.check(Act::Read, "src/main.rs").effect,
        permissive.check(Act::Read, "src/main.rs").effect,
        "escalation changed a default that was not a deny",
    );

    // **A posture still decides the defaults, and escalation runs after it.**
    // `read-only` denies writes as a tier default, so under escalation a write
    // asks — which is the posture still in force rather than overridden: an
    // operator who answers `n` is refused exactly as before.
    let read_only = approval::session_policy(&base, Some(Posture::ReadOnly), &[], true);
    assert_eq!(
        read_only.check(Act::Write, "notes.txt").effect,
        Effect::Ask,
        "a posture's own deny default did not escalate, so the two do not compose",
    );
    assert_eq!(
        read_only.check(Act::Read, ".env").effect,
        Effect::Deny,
        "and the written rule survives a posture and escalation together",
    );
}
