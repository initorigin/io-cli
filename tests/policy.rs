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

    let policy = approval::session_policy(&base, app.posture(), app.remembered());
    assert_eq!(
        policy.check(Act::Write, "src/main.rs").effect,
        Effect::Allow,
        "the workspace posture writes inside the workspace without asking",
    );

    app.key(shift_tab());
    let policy = approval::session_policy(&base, app.posture(), app.remembered());
    assert_eq!(
        policy.check(Act::Write, "src/main.rs").effect,
        Effect::Ask,
        "ask-before-writes has to actually ask",
    );

    app.key(shift_tab());
    let policy = approval::session_policy(&base, app.posture(), app.remembered());
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
        let policy = approval::session_policy(&base, Some(*posture), &[]);
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
    assert_eq!(approval::session_policy(&base, None, &[]), base);
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

/// **F1.** The posture the wizard recommends refuses git, and refuses it where no
/// approver is reachable: `Git::run` (io-harness 0.69.0, `src/tools/git.rs:549`)
/// treats every effect that is not `Allow` as a hard refusal. Asserted as
/// `!= Allow` rather than `== Ask`, because that inequality is the whole predicate
/// upstream applies.
#[test]
fn f1_an_asking_posture_refuses_git_before_anyone_is_asked() {
    let base = Policy::default();
    let policy = approval::session_policy(&base, Some(Posture::AskWrites), &[]);
    assert_ne!(
        policy.check(Act::Exec, "git").effect,
        Effect::Allow,
        "the recommended posture leaves git short of allow, which upstream refuses",
    );
    assert!(approval::refuses_git(&policy));
}

/// **F1.** The repair, offered through the mechanism that already exists for
/// everything else the operator allows for the session.
#[test]
fn f1_the_git_allowance_turns_that_refusal_into_an_allow() {
    let base = approval::session_policy(&Policy::default(), Some(Posture::AskWrites), &[]);
    let policy = approval::effective_policy(&base, &[approval::git_allowance()]);
    assert_eq!(
        policy.check(Act::Exec, "git").effect,
        Effect::Allow,
        "with the allowance in force the seven git tools may spawn",
    );
    assert!(!approval::refuses_git(&policy));
}

/// **F1, and the half the sabotage attacks.** One rule, one program. An
/// `Act::Exec` pattern names a binary, so a grant written wider than the binary it
/// was for is a grant of the whole PATH.
#[test]
fn f1_the_git_allowance_changes_nothing_for_another_program() {
    let base = approval::session_policy(&Policy::default(), Some(Posture::AskWrites), &[]);
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
        let policy = approval::session_policy(&base, Some(*posture), &[approval::git_allowance()]);
        assert_eq!(
            policy.check(Act::Exec, "git").effect,
            Effect::Deny,
            "{:?}: the allowance re-opened a spawn a layer had denied",
            posture,
        );
        assert!(approval::refuses_git(&policy));
    }
}
