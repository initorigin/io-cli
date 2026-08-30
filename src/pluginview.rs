//! `/plugin` — which capability bundles this configuration declared, what each
//! one brought, and which ones io-harness refused to load.
//!
//! A plugin is a directory with a `plugin.toml` contributing skills, templates,
//! agents, MCP servers, hooks and deny-only policy layers, and io-cli has never
//! had a surface for one. It needs one for a reason the other management panels
//! do not: **every other capability an operator sees is one they put there.** A
//! skill file is theirs or io-cli's, an `[[mcp]]` entry is a line they wrote, a
//! policy layer came from a posture they chose. A bundle is none of those — it is
//! a directory somebody else wrote, named once by path, that then adds names to
//! four subsystems at once. The question this surface exists to answer is *what
//! did that directory put in my session*, and until now the only way to answer it
//! was to open the manifest.
//!
//! # A dropped bundle is the state this surface is really for
//!
//! [`io_harness::Plugins`] has no error path. A bundle with no manifest,
//! unparseable TOML, a bad id, or a contribution its declaring scope may not make
//! is **dropped** — recorded, reported, and otherwise silently absent while every
//! other bundle loads. io-harness's own module docs call that a deliberate trade
//! with a cost: *a bundle an operator believes is loaded can be silently absent
//! for a week*. This panel is where that week ends, so the refused set is a field
//! of [`View`] rather than an error, drawn beside the loaded set and never
//! instead of it.
//!
//! **The reason is io-harness's sentence and it is carried verbatim.** It names
//! the file, names the key, and — for the project-scope refusal — explains in two
//! clauses why a `git clone`d `io.toml` may not name a program this machine will
//! run. io-cli could not write that better and must not try: a reworded refusal
//! is io-cli's opinion about somebody else's rule, and the operator who has to fix
//! it needs the words of whoever enforced it. [`rows`] may *shorten* the sentence
//! to fit a row, with the mark that says so; [`Refused::error`] holds it whole,
//! and the driver prints it whole beneath the list.
//!
//! # A switched-off bundle is declared, and this surface says so
//!
//! io-harness 0.70.0 splits what a configuration declared into **three** buckets
//! rather than two: `Plugins::iter` is what loaded, `Plugins::dropped` is what was
//! refused, and [`io_harness::Plugins::disabled`] is what was written
//! `enabled = false` — read, parsed, held to the whole trust rule, and
//! contributing nothing to any of the six kinds. That is a third *state*, never a
//! second kind of failure: everything on `dropped` is something an operator has to
//! fix, and a switched-off bundle is doing exactly what the file asked of it.
//!
//! **It is listed, under its own mark, and it counts against [`View::is_empty`].**
//! Before 0.29.0 [`view`] read `iter()` and `dropped()` and nothing else, so a
//! configuration declaring three bundles switched off drew an empty list and
//! `/plugin` said *no capability bundles are declared yet* — the section above,
//! inverted: a capability missing from every listing reads exactly like one nobody
//! ever wrote down, and the operator's own `enabled = false` is the one edit they
//! can undo in a keystroke if they can see it. What a switched-off bundle *would*
//! bring is drawn beside it for the same reason — the question an operator
//! switching one back on has is what comes back.
//!
//! **The state rides on [`Listed::enabled`] rather than a third list**, because
//! every index on this surface addresses a list somewhere else and a third one is
//! a third off-by-one — the shape of the silent wrong delete 0.20.0 shipped in
//! [`rows`]. The cost is one caller that has to remember the flag:
//! `src/main.rs`'s `bundle_skills` builds the `/skills` palette off [`view`],
//! because `Plugins::skill_dirs` is `pub(crate)`, and it filters on
//! [`Listed::enabled`] so a switched-off bundle's skills are not offered to a
//! model that cannot reach them.
//!
//! # Hooks are listed here now, and io-harness is what names them
//!
//! io-harness 0.71.0 publishes [`io_harness::Plugin::hooks`] and makes
//! [`io_harness::Hook`] public with an accessor for every key it carries, so the
//! contribution kind that **runs programs** is no longer the one kind this surface
//! could only say the word `hooks` about. Every hook row is built in
//! [`copy_out`] out of those accessors and drawn by [`detail`], and there is no
//! longer any branch that says io-cli cannot say what a hook runs — because it
//! can.
//!
//! **This module still opens no `plugin.toml`.** The hooks arrive the same way the
//! agents, the servers and the layers always have: off a [`io_harness::Plugin`]
//! io-harness read itself. That is the whole reason the reading moved rather than
//! being duplicated — io-cli's own reader (deleted in 0.30.0) could not see an
//! inline `hook = [{…}]` array or a `[[hook]]` header carrying a trailing comment,
//! and drew a bundle with hooks as a bundle with none, which is the one reading
//! that is false on the contribution kind that spawns.
//!
//! `Hook::on_failure` is read rather than the raw key, because a table that wrote
//! no `on_failure` still has one and it is its kind's — `refuse` for a lifecycle
//! hook, `continue` for an event hook. Nothing here re-derives that rule.
//!
//! # The names drawn here are already namespaced, and that is the point
//!
//! io-harness rewrites every contributed agent name, MCP server id and policy
//! layer name to `<plugin>__<name>` at load, so `agents()[0].name` is
//! `rust-review__reviewer`. This surface draws that string unchanged. It is not a
//! prettier name with the prefix stripped, because the namespaced form is what a
//! refusal will name in `PolicyEvent::layer`, what a call will name in
//! `McpEvent::server`, and what the operator will type to spawn the agent — and a
//! panel that showed a shorter name than the trace does would have invented a
//! third spelling of the same thing.
//!
//! # No TOML, no terminal I/O, no keys
//!
//! Nothing here opens a `plugin.toml`. Every fact comes from
//! [`io_harness::Config::plugins`], which re-reads each declared bundle from disk
//! itself; a second reader in this crate would be a second opinion about what a
//! manifest means, and `tests/dependencies.rs` forbids one by path.
//!
//! **One fact on this surface is not io-harness's**: whether io wrote a bundle's
//! `plugin.toml` itself. io-harness loads a generated manifest exactly as it loads
//! an authored one and has no word for the difference, so the answer is a path —
//! the directory [`crate::home::adapters`] names, resolved once in [`view`] and
//! carried on [`View::adapters`] — and it is still not a second read of any file.
//! See [`Listed::adapted`].
//!
//! Like [`crate::servers`] and [`crate::skillview`], this is a data model and
//! functions over it: [`view`] is where the machine is read, once, and everything
//! that draws is pure over what it returned. The driver in `src/main.rs` owns the
//! keyboard and applies what [`add`] and [`remove`] return.

use std::path::{Path, PathBuf};

use crate::glyphs::Glyphs;
use crate::picker::{fit, fit_left, Row};

/// The mark on a bundle that loaded.
///
/// **One ASCII character in both glyph sets, and that is the ASCII form N4 asks
/// for**, by [`crate::skillview::Origin::word`]'s argument: a mark degrades only
/// where the substitute still carries the meaning, and a pair of dots or arrows
/// that both fall back to `*` would leave an operator unable to tell a loaded
/// bundle from a refused one on the terminal that needed the fallback. `+` and
/// `!` are the marks [`crate::commands`] already uses for *present* and for
/// *attention*, they are legible in either set, so they need no set.
pub const LOADED_MARK: &str = "+";

/// The mark on a bundle that was declared and did not load. See [`LOADED_MARK`].
pub const REFUSED_MARK: &str = "!";

/// The mark on a bundle declared `enabled = false`. See [`LOADED_MARK`].
///
/// `-` against `+`, which is the one pairing that needs no legend: it is the same
/// opposition [`crate::commands`] draws between present and absent, it is one
/// ASCII character in both glyph sets, and it cannot be confused with the `!` that
/// means a bundle is broken — the distinction the whole third state exists to
/// draw, since a switched-off bundle is nothing for the operator to fix.
pub const DISABLED_MARK: &str = "-";

/// The mark on a bundle whose `plugin.toml` io wrote. See [`LOADED_MARK`].
///
/// `~` against `+`, `-` and `!`: one ASCII character in both glyph sets, on no
/// other row this surface draws, and shaped like none of the three — no stroke of
/// it is a stroke of theirs, so a column of marks stays readable at a glance
/// rather than resolving into two similar characters.
///
/// **It takes the column from the state marks rather than sitting beside them**,
/// because a row carries one mark and this is the fact an operator cannot recover
/// any other way. A bundle switched off already says "switched off" as the first
/// field of its detail — see [`rows`] — and a bundle that loaded is in the list
/// with its contributions leading the row, so the state survives the column being
/// spent here. Where the manifest came from does not survive anywhere else: an
/// adapted bundle looks exactly like a native one on every other field, and the
/// file to open when io-harness refuses it or when its contributions are not what
/// the clone promised is the generated one, which nothing else on this surface
/// names.
pub const ADAPTED_MARK: &str = "~";

/// The narrowest a bundle's root may be drawn at, before its separator.
///
/// Twenty cells is `...bundles/rust-review` — the last segments and the mark
/// saying the front went, which is what identifies a directory on a machine where
/// every bundle shares the first several segments of its path. The same floor
/// [`crate::skillview`] fits a skill file to, and for the same reason: a row may
/// lose a fact, and may not draw one that cannot be read.
const ROOT_FLOOR: usize = 20;

/// One bundle io-harness read, as `/plugin` lists it — loaded, or declared and
/// switched off.
///
/// Everything is owned and copied out of the borrowed [`io_harness::Plugin`],
/// because `Config::plugins()` returns a fresh value that re-read the disk: the
/// borrow dies with the call, and a view that held it would tie every surface to
/// the lifetime of one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The manifest's `name`, which is also what every contribution below is
    /// namespaced by.
    ///
    /// **Unique among the loaded bundles and not across the whole list.**
    /// io-harness reserves an id only for a bundle it switched on, so two
    /// bundles declared `enabled = false` may share one — which is exactly the
    /// swap the flag exists for, `tools-v1` off beside `tools-v2` on. Nothing
    /// here may key on it; [`Listed::root`] is what identifies a bundle.
    pub id: String,
    /// Whether the `[[plugin]]` entry that declared it said so, or said
    /// `enabled = false` (io-harness 0.70.0, and absent means on).
    ///
    /// **False means this bundle contributed nothing at all** — not its skills,
    /// not its agents, not its servers, not its hooks — while every field below
    /// still reads, because io-harness parses and validates a switched-off bundle
    /// in full. So they describe what switching it back on would bring, and a
    /// caller that installs any of them must check this first. See the module
    /// docs and `bundle_skills` in `src/main.rs`.
    pub enabled: bool,
    /// The manifest's one line for a human, if it carried one.
    pub description: Option<String>,
    /// The bundle's own version. **Documentation and nothing else** — io-harness
    /// resolves nothing with it and compares no two bundles by it — so it is
    /// drawn as a fact about the directory, never as a claim about compatibility.
    pub version: Option<String>,
    /// The directory the manifest was read from.
    pub root: PathBuf,
    /// Which kinds of contribution it declared, in io-harness's own fixed order:
    /// skills, templates, agents, mcp, hooks, policy.
    pub contributions: Vec<&'static str>,
    /// The skills directory it contributes, absolute, if it declared one.
    pub skills: Option<PathBuf>,
    /// The templates directory it contributes, absolute, if it declared one.
    pub templates: Option<PathBuf>,
    /// The agent names it contributes, already namespaced.
    pub agents: Vec<String>,
    /// The MCP server ids it contributes, already namespaced.
    pub servers: Vec<String>,
    /// The hooks it contributes, one `(event, command)` pair each, in the
    /// manifest's own declaration order.
    ///
    /// **Built out of [`io_harness::Hook`]'s accessors and never out of a
    /// manifest** — see the module docs. Not namespaced, and nothing io-harness
    /// namespaces: a `[[hook]]` contributes no name, it names events, a path and
    /// an argv, and all three belong to the operator's tree.
    pub hooks: Vec<(String, String)>,
    /// The policy layer names it contributes, already namespaced. Deny rules
    /// only — io-harness drops a bundle whose layer carries anything else.
    pub layers: Vec<String>,
}

impl Listed {
    /// Whether io wrote this bundle's manifest rather than the bundle's author.
    ///
    /// **A path prefix and nothing else.** [`crate::adapt::generate`] writes every
    /// manifest it generates under one directory —
    /// `<adapters>/<owner>/<repo>/<name>/plugin.toml`, the layout
    /// [`crate::adapt::at`] states — and writes generated manifests nowhere else,
    /// so a root inside `adapters` came from there and a root outside it did not.
    /// Nothing is read off the disk to answer this: the module opens no
    /// `plugin.toml` (see the module docs), and a marker inside the file would in
    /// any case be a claim about the bytes rather than about where they live.
    ///
    /// **`adapters` is io-cli's own home and does not follow `$IO_CONFIG`.**
    /// [`crate::home::adapters`] derives from the operator's home directory rather
    /// than from the configuration in force, exactly as the marketplaces do, so an
    /// operator who points `$IO_CONFIG` or `$IO_CONFIG_HOME` at an unusual place
    /// has moved their *configuration* and not the generated manifests — the
    /// prefix still matches and the mark still draws. What does change the answer
    /// is an operator who copies a generated `plugin.toml` out of that directory
    /// and declares the copy: it reads as native, which is the true answer, because
    /// the copy is a file they now own and io will not regenerate it.
    ///
    /// A method rather than a field, because [`copy_out`] is called from surfaces
    /// that have no adapters root to hand — `src/marketplace.rs` copies out a
    /// bundle it inspected inside a clone — and a field would have to be threaded
    /// through every one of them to be true anywhere.
    #[must_use]
    pub fn adapted(&self, adapters: &Path) -> bool {
        // **Both sides resolved, and `src/marketplace.rs`'s `orphaned` argues why
        // at length.** A temporary directory on macOS is reached through a
        // symlink — `/var/folders/…` and `/private/var/folders/…` are one
        // directory — and `starts_with` compares components. Comparing raw here
        // and resolved there would have `removal_cost` name a bundle as an
        // orphaned adapter while its own row drew the native mark: two surfaces,
        // two answers, one bundle.
        let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        resolve(&self.root).starts_with(resolve(adapters))
    }
}

/// One bundle that was declared and did not load.
///
/// Named for what happened rather than mirroring io-harness's `Dropped`: from
/// this side of the boundary the fact is that a configuration asked for a bundle
/// and the loader said no, and the operator's next move is in the sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The manifest's `name` where it could be read, and the directory's own name
    /// where it could not. A label, not a key — [`Refused::path`] identifies it.
    pub id: String,
    /// The directory the `[[plugin]]` entry named.
    pub path: PathBuf,
    /// io-harness's own sentence, whole. **Never reworded and never summarised**;
    /// [`rows`] may shorten it to fit, and this field is what the driver prints
    /// underneath. See the module docs.
    pub error: String,
}

/// Everything `/plugin` draws.
///
/// Two lists rather than a `Result`, for [`crate::skillview::View`]'s reason: they
/// are not alternatives. One broken bundle costs exactly itself in io-harness, so
/// a refused bundle sits beside four that loaded and both facts are true at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    /// The bundles io-harness read: the loaded ones first, in the order they were
    /// declared — which is the order their policy layers stack and their
    /// contributions are applied in — and then the ones declared
    /// `enabled = false`, each flagged by [`Listed::enabled`].
    ///
    /// **One list and a flag, not two lists.** See the module docs: a third list
    /// is a third index, and every index on this surface addresses a list
    /// somewhere else.
    pub plugins: Vec<Listed>,
    /// The bundles that did not, with the reason each one did not.
    pub refused: Vec<Refused>,
    /// Where io writes the manifests it generates, on this machine.
    ///
    /// Carried on the view rather than passed to [`rows`], because it is a fact
    /// about the machine and not about the terminal: every caller of `rows` and
    /// every caller of [`detail`] would otherwise have to know it, including the
    /// ones drawing a bundle that is not on this list at all.
    ///
    /// `None` where the operator's home directory cannot be determined, which is
    /// [`crate::home::adapters`]'s own shape and is read here as *nothing is
    /// adapted*: with no adapters root there is no directory a generated manifest
    /// could be under, so no row is marked and every bundle draws as the native
    /// one it must be.
    pub adapters: Option<PathBuf>,
}

impl View {
    /// Whether the configuration declared no bundle at all.
    ///
    /// **Both lists, because a configuration whose only bundle was refused has
    /// declared one.** `Plugins::is_empty` says nothing about `dropped()`, and a
    /// surface that read it alone would tell an operator with a broken manifest
    /// that they have no plugins — which is the false sentence this module exists
    /// to stop being told.
    ///
    /// **And a configuration whose only bundle is switched off has declared one
    /// too**, which is the same sentence with a different cause: `disabled()` is
    /// a third bucket `Plugins::is_empty` is equally silent about. Those bundles
    /// are in `plugins` flagged rather than in a list of their own, so this
    /// answers `false` for them without a third clause — and the sabotage that
    /// finds a regression here is dropping `disabled()` from [`view`], not
    /// editing this line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty() && self.refused.is_empty()
    }
}

/// One [`io_harness::Plugin`], copied out with the state io-harness kept in the
/// bucket it put it in.
///
/// `enabled` is an argument rather than something read off the plugin because
/// there is nothing on `Plugin` to read: the flag lives on the `[[plugin]]` entry
/// as `Declaration::enabled`, which is `pub(crate)`, and the *only* public signal
/// of it is which of `Plugins::iter` and `Plugins::disabled` a bundle came back
/// on. So the bucket is the fact, and [`view`] is the one place it is recorded.
///
/// **Public since 0.30.0, because a bundle nobody has declared has no bucket at
/// all.** [`io_harness::Plugins::inspect`] hands back a `Plugin` for a directory
/// that is in no configuration file, and that is what a marketplace install
/// discloses from — see [`crate::marketplace::disclosure`]. It passes `true`:
/// nothing has been written yet, so nothing has been switched off.
pub fn copy_out(plugin: &io_harness::Plugin, enabled: bool) -> Listed {
    Listed {
        id: plugin.id().to_string(),
        enabled,
        description: plugin.description().map(str::to_string),
        version: plugin.version().map(str::to_string),
        root: plugin.root().to_path_buf(),
        contributions: plugin.contributions(),
        skills: plugin.skills_dir(),
        templates: plugin.templates_dir(),
        agents: plugin.agents().iter().map(|def| def.name.clone()).collect(),
        servers: plugin
            .mcp_servers()
            .iter()
            .map(|server| server.id.clone())
            .collect(),
        hooks: plugin.hooks().iter().map(hook_line).collect(),
        layers: plugin
            .policy_layers()
            .iter()
            .map(|layer| layer.name.clone())
            .collect(),
    }
}

/// One [`io_harness::Hook`] as the two fields a reader needs: when it fires, and
/// what it then does.
///
/// **Every field comes off an accessor**, which is the whole of criterion F16: the
/// manifest is io-harness's to read, and io-cli's own reader could not see an
/// inline `hook = [{…}]` array or a `[[hook]]` header with a trailing comment on
/// it. Both are ordinary TOML, io-harness accepts both, and both used to draw as no
/// hook at all.
///
/// `on = []` is **every** event and is said so: io-harness documents an empty `on`
/// as firing on all of them, so the hook that runs most often was the one drawn as
/// the two least alarming characters on the screen where consent happens.
///
/// `on_failure` is named only where it stops something — [`io_harness::OnFailure`]
/// resolves a table that wrote nothing to its kind's own default, and a row saying
/// `continue` on every event hook is a column of noise over the one word that
/// matters. `cancel` ends the run and `refuse` stops the call, and neither is
/// something to learn afterwards.
///
/// **Both values go through `marketplace::plain` and neither is
/// shortened.** An argv element is a TOML string and TOML permits a raw newline
/// inside a `"""`, so a stranger's manifest could otherwise put forged extra lines
/// into the scrollback an operator is reading to decide. The length is not bounded,
/// because this is argv somebody is consenting to and a shortened argv is worse
/// than no argv at all.
fn hook_line(hook: &io_harness::Hook) -> (String, String) {
    let mut event = match hook.at() {
        Some(at) if hook.tools().is_empty() => at.to_string(),
        Some(at) => format!("{at} on {}", hook.tools().join(", ")),
        None if hook.on().is_empty() => "every event".to_string(),
        None => hook.on().join(", "),
    };
    match hook.on_failure() {
        io_harness::OnFailure::Cancel => event.push_str(", cancels the run if it fails"),
        io_harness::OnFailure::Refuse => event.push_str(", refuses the call if it fails"),
        _ => {}
    }
    let command = match (hook.run(), hook.append()) {
        (Some(argv), _) => format!("[{}]", argv.join(" ")),
        (None, Some(file)) => format!("appends to {}", file.display()),
        // Unreachable through `Plugins::inspect` and `Config::plugins` alike —
        // io-harness's `Hooks::check` refuses a table with neither — and said
        // rather than skipped, because a hook drawn as a blank row is the silence
        // this whole surface exists to end.
        (None, None) => "declares neither `run` nor `append`".to_string(),
    };
    (
        crate::marketplace::plain(&event),
        crate::marketplace::plain(&command),
    )
}

/// Every bundle this configuration declared: loaded, switched off, and refused.
///
/// **All three of io-harness's buckets, and the third one is why this function
/// changed in 0.29.0.** `Plugins::iter` is the loaded set alone — its own rustdoc
/// says so of `len` and `is_empty` — so a `view` reading it and `dropped()` was
/// blind to every bundle an operator had declared `enabled = false`, and drew them
/// nowhere at all. See the module docs.
///
/// Infallible, because [`io_harness::Config::plugins`] is: it re-reads every
/// declared directory from disk on the call and reports what it could not read
/// rather than returning an error. That re-read is deliberate here — the surface
/// an operator opens after editing a manifest must see the edit, and a set cached
/// at session start would not.
#[must_use]
pub fn view(config: &io_harness::Config) -> View {
    let plugins = config.plugins();
    View {
        // Loaded first and switched off after, which is the order [`rows`] draws
        // and the order the positional contract there is written against.
        plugins: plugins
            .iter()
            .map(|plugin| copy_out(plugin, true))
            .chain(
                plugins
                    .disabled()
                    .iter()
                    .map(|plugin| copy_out(plugin, false)),
            )
            .collect(),
        refused: plugins
            .dropped()
            .iter()
            .map(|dropped| Refused {
                id: dropped.id.clone(),
                path: dropped.path.clone(),
                error: dropped.error.clone(),
            })
            .collect(),
        // Resolved once, here, where the view is assembled. The alternative is
        // every drawing function calling [`crate::home::adapters`] itself, which
        // would make three pure functions read this machine's environment and
        // leave a test unable to say what any of them saw.
        adapters: crate::home::adapters(),
    }
}

/// The picker rows for the whole list, fitted for a terminal this wide.
///
/// **One row per bundle, `view.plugins` first and refused after, and no
/// headings.** The picker hands a chosen index straight back into the list, so a
/// heading row is an index that maps to no bundle — [`crate::commands`] carries a
/// parallel vector of `Held::Nothing` to survive that, and this surface has two
/// lists to index rather than one. So the contract is positional and written down:
/// **index `i` is `view.plugins[i]` while `i < view.plugins.len()`, and
/// `view.refused[i - view.plugins.len()]` after it.** The mark, not a heading,
/// says which — and unlike a heading it survives a typed query, which is exactly
/// when a refused bundle is hardest to tell from a loaded one.
///
/// **A bundle declared `enabled = false` is inside `view.plugins` and so is inside
/// that first range**, drawn under [`DISABLED_MARK`] with the state leading its
/// detail. Four marks and still two ranges: the switched-off bundles are a state
/// the operator chose, not a second failure list, and giving them a range of their
/// own would have added a third arm to every index calculation on this surface for
/// nothing.
///
/// **The fourth mark is [`ADAPTED_MARK`], and it takes the column from the other
/// two rather than adding a range of its own.** A bundle whose manifest io
/// generated is an ordinary declared bundle — it loads, it can be switched off, it
/// is indexed exactly where it sits — so the only thing that changes is which
/// character the row wears. See [`ADAPTED_MARK`] for why that character is worth
/// more there than `+` or `-` is, and [`Listed::adapted`] for what decides it.
///
/// # The narrow form, and what gives way in it
///
/// The id is the label. The contributions are unconditional, because they are the
/// answer to what the bundle *did to this session* and are the only field here
/// with no other home. The version follows when there is one, the description
/// takes what is left, and **the root path is the field that gives way** — below
/// a floor this module keeps privately it is dropped whole rather than drawn as an
/// ellipsis and an extension. At eighty columns a row is therefore the id, what it
/// contributed and
/// as much description as fits; widen the terminal and the path arrives.
///
/// **On an adapted bundle that trade is reversed**: the root's floor is reserved
/// ahead of the description and the description gives way instead. The row's mark
/// says io wrote the manifest and the row is then the only place that says where
/// it wrote it — see [`ADAPTED_MARK`].
///
/// A refused row never carries the root, and not because it did not fit:
/// io-harness's sentence already opens with the path, so appending it would draw
/// the same string twice on the row with the least room to spare.
pub fn rows(view: &View, width: u16, glyphs: &Glyphs) -> Vec<Row> {
    let separator = glyphs.separator;
    let separator_width = separator.chars().count();
    // Worth appending only if there is room for more than the mark that would say
    // it had been shortened, measured off the set: the ellipsis is one cell in
    // Unicode and three in ASCII.
    let floor = separator_width + glyphs.ellipsis.chars().count();

    let mut out = Vec::with_capacity(view.plugins.len() + view.refused.len());
    let adapters = view.adapters.as_deref();

    for plugin in &view.plugins {
        let adapted = adapters.is_some_and(|root| plugin.adapted(root));
        // Commas inside a field, the glyph set's separator between fields: this
        // is one fact — the list of kinds — and punctuating it with the same run
        // that divides the row would read as five fields.
        let mut detail = plugin.contributions.join(", ");
        if !plugin.enabled {
            // **The state leads the row, and the mark is not left to carry it
            // alone.** A single character says which bucket a row came from; it
            // does not say that `skills, agents` is a list of things this session
            // does *not* have. Reading the kinds first on a switched-off bundle is
            // the one wrong sentence this row can tell, so the words go in front
            // of them — two words and not a clause, because the contributions are
            // unconditional here and every cell spent in front of them is a cell
            // the eighty-column row (N4) has to find somewhere else.
            detail = if detail.is_empty() {
                "switched off".to_string()
            } else {
                format!("switched off{separator}{detail}")
            };
        } else if detail.is_empty() {
            // A manifest with a `name` and nothing else. Loaded, contributing
            // nothing, and saying so is the whole point of the row.
            detail = "contributes nothing".to_string();
        }
        if let Some(version) = &plugin.version {
            detail.push_str(separator);
            detail.push_str(version);
        }

        // The picker's own arithmetic, mirrored rather than guessed: two cells of
        // marker, the label, two cells of gap. A budget one cell out is how an
        // ellipsis ends up on the floor.
        let mut left = (width as usize)
            .saturating_sub(4)
            .saturating_sub(plugin.id.chars().count())
            .saturating_sub(detail.chars().count());

        // **On an adapted bundle the root is not the field that gives way.** It is
        // the directory holding the manifest io wrote, which is the file to open
        // when io-harness refuses the bundle or when what it contributed is not
        // what the clone promised — and nothing else on this surface names it. The
        // description competing with it is a line io copied out of somebody else's
        // metadata, so the root's floor is reserved before the description is
        // fitted rather than taken from whatever the description leaves.
        let reserved = if adapted {
            separator_width + ROOT_FLOOR
        } else {
            0
        };
        if let Some(description) = &plugin.description {
            if left > floor + reserved {
                let described = fit(description, left - separator_width - reserved, glyphs);
                left -= separator_width + described.chars().count();
                detail.push_str(separator);
                detail.push_str(&described);
            }
        }
        if left >= separator_width + ROOT_FLOOR {
            detail.push_str(separator);
            // From the left, because every bundle on one machine shares the first
            // several segments of its path and the end is what identifies it.
            detail.push_str(&fit_left(
                &plugin.root.display().to_string(),
                left - separator_width,
                glyphs,
            ));
        }

        out.push(Row::marked(
            if adapted {
                ADAPTED_MARK
            } else if plugin.enabled {
                LOADED_MARK
            } else {
                DISABLED_MARK
            },
            plugin.id.clone(),
            detail,
        ));
    }

    for refused in &view.refused {
        let mut detail = String::from("not loaded");
        let left = (width as usize)
            .saturating_sub(4)
            .saturating_sub(refused.id.chars().count())
            .saturating_sub(detail.chars().count());
        if left > floor {
            detail.push_str(separator);
            // Shortened from the head like any other prose, with the mark that
            // says so. `Refused::error` still holds the sentence whole, and the
            // driver prints it whole — a row is a pointer, not the report.
            detail.push_str(&fit(&refused.error, left - separator_width, glyphs));
        }
        out.push(Row::marked(REFUSED_MARK, refused.id.clone(), detail));
    }

    out
}

/// The picker rows for one bundle: what it actually put into the session.
///
/// Grouped under headings, which are free here and are not free in [`rows`]: this
/// list is read rather than chosen from, so an index that maps to no bundle costs
/// nothing. The order is io-harness's own contribution order, so a bundle reads
/// the same way here as it does in the trace.
///
/// Every name is drawn **namespaced, exactly as io-harness rewrote it** — see the
/// module docs.
///
/// A switched-off bundle gets the same groups with one row above them saying that
/// none of it is in this session. Every accessor is valid on one — io-harness
/// parses and validates a disabled bundle in full — so the groups are true about
/// the *directory* and false about the *session*, and the row above them is what
/// makes the pane say which. See [`Listed::enabled`].
///
/// # `hooks` is a group like any other now
///
/// It comes off [`io_harness::Plugin::hooks`] through [`copy_out`], exactly as the
/// agents and the servers do, so there is no argument to pass and no caller that
/// can be handed a bundle's hooks and forget to. The placeholder row that said
/// io-cli could not name a hook is gone with the gap it described — see the module
/// docs.
pub fn detail(plugin: &Listed, width: u16, glyphs: &Glyphs) -> Vec<Row> {
    // The picker's own arithmetic again: marker and gap, with no detail column to
    // budget for on the rows that are a path.
    let room = (width as usize).saturating_sub(4);
    let mut out = Vec::new();

    if let Some(dir) = &plugin.skills {
        out.push(Row::heading("skills"));
        out.push(Row::new(fit_left(&dir.display().to_string(), room, glyphs)));
    }
    if let Some(dir) = &plugin.templates {
        out.push(Row::heading("prompt templates"));
        out.push(Row::new(fit_left(&dir.display().to_string(), room, glyphs)));
    }
    if !plugin.agents.is_empty() {
        out.push(Row::heading("agents"));
        out.extend(plugin.agents.iter().map(|name| Row::new(name.clone())));
    }
    if !plugin.servers.is_empty() {
        out.push(Row::heading("mcp servers"));
        out.extend(plugin.servers.iter().map(|id| Row::new(id.clone())));
    }
    if !plugin.hooks.is_empty() {
        // **One row per hook, naming the event and the command.** A bundle's hooks
        // are the contribution that runs programs, and "hooks" is the one word on
        // this pane that tells an operator nothing about what they are consenting
        // to. Every field is io-harness's — see `hook_line`.
        out.push(Row::heading("hooks"));
        out.extend(plugin.hooks.iter().map(|(event, command)| {
            Row::with_detail(
                event.clone(),
                fit(
                    command,
                    room.saturating_sub(event.chars().count() + 2),
                    glyphs,
                ),
            )
        }));
    }
    if !plugin.layers.is_empty() {
        out.push(Row::heading("policy layers, deny only"));
        out.extend(plugin.layers.iter().map(|name| Row::new(name.clone())));
    }

    if out.is_empty() {
        // A manifest that declared a name and nothing else. It loaded, so it is
        // in the list; this is the row that says the list entry is all there is.
        out.push(Row::new("this bundle contributes nothing"));
    }
    if !plugin.enabled {
        // **First, because every row under it is false without it.** io-harness
        // parses and validates a switched-off bundle in full, so the groups above
        // are the real agents, servers and layers of a real directory — and this
        // session has none of them. A pane that opened on `agents` over a bundle
        // contributing no agent is the same silent wrong reading the module docs
        // exist to end, told the other way round. `insert` rather than a branch
        // around the build: the groups are what switching it on brings back, and
        // an operator deciding whether to do that needs to see them.
        out.insert(
            0,
            Row::with_detail(
                "switched off",
                fit(
                    "this bundle's `[[plugin]]` entry says `enabled = false`, so none of what \
                     follows is in this session; it is what switching it back on would bring",
                    room.saturating_sub("switched off".chars().count() + 2),
                    glyphs,
                ),
            ),
        );
    }
    out
}

/// The edit that declares a bundle.
///
/// A whole `[[plugin]]` entry, because an array of tables grows by gaining a block
/// and [`crate::edit::Edit::set`] can only reach inside one that exists — the same
/// shape [`crate::servers::add`] writes for `[[mcp]]`.
///
/// The path is written as given. **A relative one resolves against the discovery
/// root**, not against the file that named it, so io-cli neither absolutises it
/// here nor tidies it: a relative path in a committed `io.toml` is how a bundle
/// vendored into a repository is shared, and rewriting it to this machine's
/// absolute path would break that for everyone else who clones it.
pub fn add(dir: &Path) -> crate::edit::Edit {
    crate::edit::Edit::append(
        "plugin",
        format!("path = {}", quoted(&dir.display().to_string())),
    )
}

/// What writing an `enabled` key into a `[[plugin]]` costs an older binary.
///
/// **`enabled` on a `[[plugin]]` is io-harness 0.70.0's, and 0.69.0 does not
/// ignore it — it refuses the whole file.** The `[[mcp]]` case is the opposite and
/// that is exactly why this has to be said out loud: an operator who has seen a
/// forward-compatible key before will assume this one is too. A shared `io.toml`
/// or a second machine on the older binary loses every setting in the file, not
/// the bundle.
pub const OLDER_BINARY: &str =
    "this writes an `enabled` key into a `[[plugin]]` entry, which is io-harness 0.70.0's: an \
     io-cli built against 0.69.0 refuses the whole configuration file rather than ignoring the \
     key";

// **`add_off` and `declared_off` were deleted in 0.30.0.**
//
// Through 0.29.0 a marketplace install wrote `[[plugin]] enabled = false` and
// then read the entry back, because io-harness published no loader that took a
// directory: the only way to have a stranger's bundle read, parsed, validated
// and trust-checked was to *declare* it, and the only way to declare it without
// running it was that key. `add_off` wrote it and `declared_off` recovered which
// entry had just been written.
//
// `io_harness::Plugins::inspect` (0.71.0, io-harness#224) validates a directory
// with nothing on disk naming it, so the install validates first and writes once,
// on consent, through [`add`] — and both functions lost their only callers in the
// same change. They are deleted rather than left `pub`, because a public function
// no keystroke reaches is the defect shape this product has shipped six times, and
// this release closed five of them.
//
// `tests/marketplace.rs` still asserts that `src/manage.rs` names no `add_off`;
// it now asserts the stronger thing, that nothing does.

/// The edit that switches the `index`-th declared bundle on.
///
/// **One key and no other byte of the file.** [`crate::edit::Edit::set`] resolves
/// `plugin[N].enabled` through the same splitter every other path goes through —
/// `plugin[N]` is the section, `enabled` is the key — and replaces the value's own
/// span, so the path this entry declares, every sibling entry, every comment and
/// every unrelated section come through untouched. Rewriting the entry instead
/// would be the same configuration expressed in different bytes, and an operator
/// who diffs their `io.toml` after consenting would see io-cli reformat a file
/// they never asked it to touch.
///
/// `index` is an index into the file's `[[plugin]]` array, with [`remove`]'s own
/// warning: it is not a row number from [`rows`].
pub fn enable(index: usize) -> crate::edit::Edit {
    crate::edit::Edit::set(format!("plugin[{index}].enabled"), "true")
}

/// The edit that switches the `index`-th declared bundle **off**.
///
/// [`enable`]'s write with the other value, and everything that function says
/// about one key and no other byte holds here word for word.
///
/// **It is not [`remove`], and the difference is the whole verb.** A removal takes
/// the `[[plugin]]` entry away: the bundle stops being declared, disappears from
/// [`rows`] entirely, and switching it back on means finding the directory again
/// and typing the path. This leaves the entry exactly where it is with one word
/// changed, so the bundle goes on being listed — under [`DISABLED_MARK`], with
/// "switched off" leading its detail — and the way back on is the same keystroke
/// again. An operator who wanted a bundle *gone* has `remove`; one who wanted it
/// *quiet* had nothing until this.
///
/// Says [`OLDER_BINARY`] to the operator at the moment of writing — and that
/// sentence is the `[[plugin]]` one, which is not
/// [`crate::servers::OLDER_BINARY`]. A 0.69.0 binary refuses this whole file; it
/// ignores the same key in an `[[mcp]]` entry and runs the server.
///
/// `index` is an index into the file's `[[plugin]]` array, with [`remove`]'s own
/// warning: it is not a row number from [`rows`].
pub fn disable(index: usize) -> crate::edit::Edit {
    crate::edit::Edit::set(format!("plugin[{index}].enabled"), "false")
}

/// The manifest every bundle has, and the one file this surface can check before
/// writing anything.
///
/// **io-harness's own constant, re-exported rather than spelled.** The name a
/// directory is recognised by is the dependency's to state; a literal here would
/// go on matching `plugin.toml` through a release that renamed it, and every check
/// below would then answer confidently about a file io-harness no longer reads.
pub const MANIFEST: &str = io_harness::PLUGIN_FILE;

/// How deep [`candidates`] looks for a bundle below the discovery root.
///
/// Three, because a bundle is conventionally vendored one or two directories down
/// — `bundles/rust-review`, `vendor/plugins/rust-review` — and a walk that keeps
/// going is a walk that reads a whole `target/` on a settings screen. An operator
/// whose bundle lives deeper still has the typed path, which is refused by the
/// same check rather than by a shallower one.
const DEPTH: usize = 3;

/// Why `dir` is not a bundle, or `None` when it is one.
///
/// **The check before the write, and the reason it exists is that io-harness has
/// no error path for this.** `Config::plugins()` is infallible and a `[[plugin]]`
/// entry naming a directory with no manifest is *dropped* — recorded and otherwise
/// silently absent — so an entry written without this check produces exactly the
/// state the module docs above call a bundle an operator believes is loaded and
/// which is silently absent for a week. The refusal names the directory and the
/// file it looked for, because "not a bundle" tells an operator nothing about what
/// to do next.
///
/// Deliberately the *only* thing checked. Whether the manifest parses, declares a
/// usable `name`, or may make its contributions in the scope it is being declared
/// in are all io-harness's questions, and it answers them with sentences this
/// surface carries verbatim rather than paraphrasing. Existence is the one part
/// io-harness cannot report before the entry exists.
#[must_use]
pub fn refusal(dir: &Path) -> Option<String> {
    if dir.join(MANIFEST).is_file() {
        return None;
    }
    Some(if dir.is_dir() {
        format!(
            "{} has no {MANIFEST}, so it is not a capability bundle; nothing was written",
            dir.display()
        )
    } else {
        format!("{} is not a directory; nothing was written", dir.display())
    })
}

/// The directories below `root` that carry a [`MANIFEST`], nearest first.
///
/// **A list to choose from rather than a path to remember, which is the whole
/// point of the verb.** The alternative — a composer prefilled with `/plugin add `
/// — asks an operator to type a path they have to go and look up, and mistypes it
/// into an entry that is then silently dropped.
///
/// Ordered by depth and then by path so the answer is stable between two calls on
/// one machine, which is what makes the row an operator picked yesterday the row
/// in the same place today. `target`, `node_modules` and every dotted directory
/// are skipped: none of them is somewhere a person puts a bundle, and `target` is
/// the one that would make this walk expensive.
#[must_use]
pub fn candidates(root: &Path) -> Vec<PathBuf> {
    visited(root)
        .into_iter()
        .filter(|path| path.join(MANIFEST).is_file())
        .collect()
}

/// Every directory below `root` the bundle walk looks at, nearest first.
///
/// **[`candidates`] is this filtered by "carries a [`MANIFEST`]", and separating
/// the two is what 0.31.0 needed rather than a second walk.** A directory holding
/// only a `.claude-plugin/plugin.json` carries no `plugin.toml`, so it was never
/// in `candidates`'s answer at all — a marketplace reader that iterated that list
/// looking for foreign manifests would have found none, ever, and the walk it
/// wanted was the frontier rather than the result.
///
/// The skips are unchanged and they are the point: `target`, `node_modules` and
/// **every dotted directory** are never entered, so `.git` — which every clone has
/// — is not walked here and cannot contribute a bundle. Reading
/// `.claude-plugin/plugin.json` at a directory this function returned is a known
/// path relative to an already-admitted directory and is not this walk descending
/// into a dot directory; `tests/marketplace.rs` asserts the distinction.
///
/// Ordered by depth and then by path so the answer is stable between two calls on
/// one machine, which is what makes the row an operator picked yesterday the row
/// in the same place today.
#[must_use]
pub fn visited(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut frontier = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = frontier.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            found.push(path.clone());
            if depth + 1 < DEPTH {
                frontier.push((path, depth + 1));
            }
        }
    }
    found.sort_by_key(|path| (path.components().count(), path.clone()));
    found
}

/// How a chosen directory is written into the entry.
///
/// Relative to `root` when it sits below it, absolute otherwise — and that is not
/// a tidying preference, it is the difference between a bundle a repository can
/// share and one only this machine can load. [`add`] writes the path as given and
/// a relative one resolves against the discovery root, so a `bundles/rust-review`
/// in a committed `io.toml` works for everyone who clones it while this machine's
/// `/Users/…/bundles/rust-review` works for nobody else.
#[must_use]
pub fn declared(root: &Path, dir: &Path) -> PathBuf {
    dir.strip_prefix(root).map_or_else(
        |_| dir.to_path_buf(),
        |relative| {
            // A directory that *is* the root strips to nothing, and an empty path
            // in an entry names no directory at all.
            if relative.as_os_str().is_empty() {
                dir.to_path_buf()
            } else {
                relative.to_path_buf()
            }
        },
    )
}

/// The edit that removes the `index`-th declared bundle whole.
///
/// By index rather than by id, because the id is the *manifest's* and the entry is
/// the *configuration's*: a bundle that was refused for having an unusable `name`
/// has no id to remove it by, and it is exactly the entry an operator wants gone.
///
/// **`index` is an index into the file's `[[plugin]]` array and into nothing
/// else.** In particular it is *not* a row number from [`rows`], which lists the
/// loaded bundles and then the refused ones — two lists whose order has no
/// relation to the order the entries appear in any file. Handing a row number to
/// this function deletes a different bundle than the one on screen. Find the entry
/// with [`declared_at`], which answers the question this argument asks.
pub fn remove(index: usize) -> crate::edit::Edit {
    crate::edit::Edit::remove(format!("plugin[{index}]"))
}

/// Which scope file declares `root`, and at which `[[plugin]]` index.
///
/// **The bridge between a row on screen and an entry in a file, and it exists
/// because there is no other honest way across.** `Config::plugins()` hands back
/// loaded bundles and dropped ones as two lists, in neither case ordered by the
/// file that named them, and it says nothing at all about which of the three
/// scopes carried a given entry. So the only thing a row and an entry genuinely
/// share is the path — which is why this matches on that and refuses to guess when
/// it cannot.
///
/// Each scope's file is read and its `plugin[i].path` values are compared, both
/// as written and resolved against `root`, because a declaration may be relative
/// or absolute and both name the same directory. The first scope that matches wins,
/// searched local-first — the same precedence order the harness itself applies, so
/// a bundle declared twice is removed from the file that was actually deciding.
///
/// `None` means no file names that path, and the caller must say so rather than
/// removing something. Deleting the wrong `[[plugin]]` entry is silent: the
/// operator loses a bundle they never mentioned and finds out when its skills stop
/// being offered.
///
/// The value is **decoded before it is compared**, by the inverse of the function
/// that wrote it — see `unquoted`. Until 0.29.0 it was `trim_matches('"')`, which
/// undoes no escape at all, so a bundle at `plugins/a\b` — legal on Linux, and a
/// directory inside a clone is named by whoever wrote the clone rather than by
/// [`crate::fetch::resolve`], whose alphabet governs only `<owner>/<repo>` — was
/// written with its backslash doubled and read back with it still doubled: a
/// different path, matched against nothing, and `/plugin remove` refusing a row
/// that is plainly on screen.
pub fn declared_at(root: &Path, bundle: &Path) -> Option<(io_harness::config::Scope, usize)> {
    use io_harness::config::Scope;
    for scope in [Scope::Local, Scope::Project, Scope::User] {
        let Some(path) = crate::configure::scope_path(root, scope) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // The array is walked until a gap, which is what `value_at` reports by
        // returning `None` — an array of tables is contiguous, so the first miss
        // is the end of it.
        for index in 0.. {
            let Some(raw) = crate::edit::value_at(&text, &format!("plugin[{index}].path")) else {
                break;
            };
            let declared = PathBuf::from(unquoted(&raw));
            let resolved = if declared.is_absolute() {
                declared.clone()
            } else {
                root.join(&declared)
            };
            if declared == bundle || resolved == bundle {
                return Some((scope, index));
            }
        }
    }
    None
}

/// A TOML basic string, escaped.
///
/// The twin of [`crate::servers`]'s own, which is private to that module — here
/// rather than shared because a value is the one thing `toml::to_string` cannot be
/// asked for on its own, and because this crate writes VALUES. It earns its keep
/// on Windows, where a bundle's path is full of backslashes that a bare `"{}"`
/// would hand to the parser as escapes.
///
/// Its inverse is `unquoted`, and the two are edited together — a write escaping
/// something the read does not undo is a path that comes back as a different path.
fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// The inverse of [`quoted`]: the path back out of the value a file holds.
///
/// **The read half of this module did not exist until 0.29.0.** Both readers of
/// the day — [`declared_at`], and a `declared_off` deleted in 0.30.0 — took the
/// value's bytes and stripped the
/// surrounding quotes with `trim_matches('"')`, which undoes no escape, so every
/// path `quoted` had escaped came back doubled. On Windows the doubled separators
/// collapse and nothing shows; on Linux a bundle directory named `a\b` or `a"b` —
/// and the directories inside a clone are named by whoever wrote the clone — came
/// back as a directory that does not exist.
///
/// **Exactly `quoted`'s two escapes, undone, and no others.** A full TOML
/// basic-string decoder — the newline, tab and unicode escapes among them — would
/// be a second and wider
/// grammar for a string this module spelled itself, and TOML's grammar belongs to
/// `src/edit.rs` by rule (`tests/dependencies.rs`). A hand-written entry carrying
/// an escape `quoted` never writes therefore still comes back with the escape
/// consumed rather than expanded, which is a path that names nothing and is
/// reported as one — the same outcome as before, on a value neither reader is for.
///
/// `unquoted(&quoted(text)) == text` for every `text`, which is the property the
/// unit test below asserts and the only one either caller needs.
fn unquoted(raw: &str) -> String {
    let inner = raw.trim();
    let inner = inner.strip_prefix('"').unwrap_or(inner);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut glyphs = inner.chars();
    while let Some(glyph) = glyphs.next() {
        // A backslash takes the character after it literally, which undoes both of
        // `quoted`'s escapes in one arm. A trailing lone backslash — which `quoted`
        // cannot have written — keeps itself rather than vanishing.
        out.push(if glyph == '\\' {
            glyphs.next().unwrap_or('\\')
        } else {
            glyph
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::{ASCII, UNICODE};

    /// A bundle with something of every kind that can be listed.
    fn listed() -> Listed {
        Listed {
            id: "rust-review".to_string(),
            enabled: true,
            description: Some("Everything our Rust reviews need.".to_string()),
            version: Some("1.2.0".to_string()),
            root: PathBuf::from("/Users/someone/code/io-cli/bundles/rust-review"),
            contributions: vec!["skills", "agents", "hooks", "policy"],
            skills: Some(PathBuf::from(
                "/Users/someone/code/io-cli/bundles/rust-review/skills",
            )),
            templates: None,
            agents: vec!["rust-review__reviewer".to_string()],
            servers: Vec::new(),
            hooks: vec![("tool_call".to_string(), "[cargo fmt]".to_string())],
            layers: vec!["rust-review__no-secrets".to_string()],
        }
    }

    fn view_of(plugin: Listed) -> View {
        View {
            plugins: vec![plugin],
            refused: Vec::new(),
            // No adapters root, so every bundle here is native and the state marks
            // are what the rows wear. The adapted case is asserted in
            // `tests/plugins.rs`, against a bundle whose root really is under one.
            adapters: None,
        }
    }

    #[test]
    fn a_wide_row_carries_every_fact_in_both_glyph_sets() {
        for glyphs in [&UNICODE, &ASCII] {
            let rows = rows(&view_of(listed()), 200, glyphs);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].label, "rust-review");
            assert_eq!(rows[0].mark, Some(LOADED_MARK));
            let detail = rows[0].detail.clone().expect("a listed bundle has detail");
            assert!(
                detail.starts_with("skills, agents, hooks, policy"),
                "{}: the contributions lead the row: {detail}",
                glyphs.name
            );
            assert!(detail.contains("1.2.0"), "{}: {detail}", glyphs.name);
            assert!(
                detail.contains("Everything our Rust reviews need."),
                "{}: {detail}",
                glyphs.name
            );
            assert!(
                detail.contains("bundles/rust-review"),
                "{}: a wide row keeps the root: {detail}",
                glyphs.name
            );
            assert!(
                detail.contains(glyphs.separator),
                "{}: fields are divided by the set's own separator: {detail}",
                glyphs.name
            );
        }
    }

    #[test]
    fn at_eighty_columns_the_root_is_what_gives_way() {
        for glyphs in [&UNICODE, &ASCII] {
            let rows = rows(&view_of(listed()), 80, glyphs);
            let detail = rows[0].detail.clone().expect("a listed bundle has detail");
            assert!(
                detail.starts_with("skills, agents, hooks, policy"),
                "{}: the contributions never give way: {detail}",
                glyphs.name
            );
            assert!(
                !detail.contains("/Users/someone"),
                "{}: the root is dropped rather than drawn illegibly: {detail}",
                glyphs.name
            );
            // The picker's own budget: marker, label, gap, detail.
            assert!(
                rows[0].label.chars().count() + detail.chars().count() + 4 <= 80,
                "{}: the row fits: {detail}",
                glyphs.name
            );
        }
    }

    #[test]
    fn a_bundle_that_contributes_nothing_says_so_rather_than_drawing_an_empty_field() {
        let mut plugin = listed();
        plugin.contributions = Vec::new();
        let rows = rows(&view_of(plugin), 120, &UNICODE);
        let detail = rows[0].detail.clone().expect("a listed bundle has detail");
        assert!(detail.starts_with("contributes nothing"), "{detail}");
    }

    /// The three states are three marks, and a switched-off bundle leads with the
    /// state rather than with a list of contributions this session does not have.
    ///
    /// Sabotage: draw the disabled rows under `LOADED_MARK`. Under it the list
    /// says four bundles are loaded when one of them contributes nothing, and the
    /// operator's `enabled = false` is invisible on the surface that exists to
    /// show what is declared.
    #[test]
    fn a_switched_off_bundle_is_marked_apart_from_both_loaded_and_refused() {
        let mut off = listed();
        off.id = "tools-v1".to_string();
        off.enabled = false;
        let view = View {
            plugins: vec![listed(), off],
            refused: vec![Refused {
                id: "empty".to_string(),
                path: PathBuf::from("/repo/bundles/empty"),
                error: "/repo/bundles/empty: no plugin.toml".to_string(),
            }],
            adapters: None,
        };
        for glyphs in [&UNICODE, &ASCII] {
            let rows = rows(&view, 200, glyphs);
            assert_eq!(rows.len(), 3, "{}", glyphs.name);
            // Three states, three marks, and no two of them the same — which is
            // the whole of what a mark on this surface has to do.
            assert_eq!(rows[0].mark, Some(LOADED_MARK), "{}", glyphs.name);
            assert_eq!(rows[1].mark, Some(DISABLED_MARK), "{}", glyphs.name);
            assert_eq!(rows[2].mark, Some(REFUSED_MARK), "{}", glyphs.name);
            assert_eq!(rows[1].label, "tools-v1", "{}", glyphs.name);
            let detail = rows[1]
                .detail
                .clone()
                .expect("a disabled bundle has detail");
            assert_eq!(
                detail.split(glyphs.separator).next(),
                Some("switched off"),
                "{}: the state is not the first field of the row: {detail}",
                glyphs.name,
            );
            assert!(
                detail.contains("skills, agents, hooks, policy"),
                "{}: what switching it back on would bring is not on the row: {detail}",
                glyphs.name,
            );
        }
    }

    /// A switched-off bundle with nothing to contribute says only that it is off,
    /// rather than saying it twice in two different phrasings.
    #[test]
    fn a_switched_off_bundle_with_no_contributions_says_only_that_it_is_off() {
        let mut off = listed();
        off.enabled = false;
        off.contributions = Vec::new();
        let rows = rows(&view_of(off), 120, &UNICODE);
        let detail = rows[0]
            .detail
            .clone()
            .expect("a disabled bundle has detail");
        assert!(detail.starts_with("switched off"), "{detail}");
        assert!(
            !detail.contains("contributes nothing"),
            "the loaded-and-empty phrasing is drawn over a bundle it is not \
             about: {detail}",
        );
    }

    /// The detail pane of a switched-off bundle opens on the fact that none of it
    /// is in the session, above the groups that would otherwise read as a list of
    /// what this session has.
    ///
    /// Sabotage: drop the `insert(0, …)`. Under it the pane opens on `skills` and
    /// a namespaced agent name for a bundle contributing neither, which is the
    /// module's own silent-wrong-reading defect told the other way round.
    #[test]
    fn the_detail_view_of_a_switched_off_bundle_says_none_of_it_is_in_the_session() {
        let mut off = listed();
        off.enabled = false;
        for glyphs in [&UNICODE, &ASCII] {
            let rows = detail(&off, 120, glyphs);
            assert_eq!(rows[0].label, "switched off", "{}", glyphs.name);
            let said = rows[0]
                .detail
                .clone()
                .expect("the switched-off row explains itself");
            assert!(
                said.contains("enabled = false"),
                "{}: the row names the key an operator would edit: {said}",
                glyphs.name,
            );
            // And the groups are still drawn, because they are what switching it
            // back on brings back.
            assert!(
                rows.iter().any(|row| row.label == "agents"),
                "{}: the contributions are still shown",
                glyphs.name,
            );
            // The loaded pane has no such row, so the sentence is never drawn over
            // a bundle it is not true of.
            assert!(
                detail(&listed(), 120, glyphs)
                    .iter()
                    .all(|row| row.label != "switched off"),
                "{}",
                glyphs.name,
            );
        }
    }

    #[test]
    fn a_refused_bundle_is_marked_apart_and_carries_the_harness_sentence() {
        let sentence = "/repo/bundles/empty: no plugin.toml; a plugin is a directory with a \
                        manifest at its root";
        let view = View {
            plugins: vec![listed()],
            refused: vec![Refused {
                id: "empty".to_string(),
                path: PathBuf::from("/repo/bundles/empty"),
                error: sentence.to_string(),
            }],
            adapters: None,
        };
        for glyphs in [&UNICODE, &ASCII] {
            let rows = rows(&view, 200, glyphs);
            // The positional contract `rows` documents: loaded first, refused
            // after, one row each and nothing in between.
            assert_eq!(rows.len(), 2, "{}", glyphs.name);
            assert_eq!(rows[0].mark, Some(LOADED_MARK), "{}", glyphs.name);
            assert_eq!(rows[1].mark, Some(REFUSED_MARK), "{}", glyphs.name);
            assert_eq!(rows[1].label, "empty");
            let detail = rows[1].detail.clone().expect("a refused bundle has detail");
            assert!(detail.starts_with("not loaded"), "{detail}");
            assert!(
                detail.contains(sentence),
                "{}: the sentence is carried verbatim where it fits: {detail}",
                glyphs.name
            );
        }
    }

    #[test]
    fn a_refused_row_is_shortened_rather_than_reworded() {
        let sentence = "/repo/bundles/x: key `mcp`: a plugin declared in a project-scoped \
                        `io.toml` may not contribute `[[mcp]]`";
        let view = View {
            plugins: Vec::new(),
            refused: vec![Refused {
                id: "x".to_string(),
                path: PathBuf::from("/repo/bundles/x"),
                error: sentence.to_string(),
            }],
            adapters: None,
        };
        for glyphs in [&UNICODE, &ASCII] {
            let rows = rows(&view, 80, glyphs);
            let detail = rows[0].detail.clone().expect("a refused bundle has detail");
            assert!(
                detail.ends_with(glyphs.ellipsis),
                "{}: a shortened sentence carries the set's own mark: {detail}",
                glyphs.name
            );
            assert!(
                sentence.starts_with(
                    detail
                        .trim_start_matches("not loaded")
                        .trim_start_matches(glyphs.separator)
                        .trim_end_matches(glyphs.ellipsis)
                ),
                "{}: what is drawn is a prefix of the harness's own words: {detail}",
                glyphs.name
            );
            assert!(
                rows[0].label.chars().count() + detail.chars().count() + 4 <= 80,
                "{}: the row fits: {detail}",
                glyphs.name
            );
        }
    }

    #[test]
    fn the_detail_view_groups_what_a_bundle_contributed() {
        for glyphs in [&UNICODE, &ASCII] {
            let rows = detail(&listed(), 100, glyphs);
            let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
            assert!(labels.contains(&"skills"), "{}: {labels:?}", glyphs.name);
            assert!(labels.contains(&"agents"), "{}: {labels:?}", glyphs.name);
            assert!(
                labels.contains(&"rust-review__reviewer"),
                "{}: the namespaced name io-harness wrote, not a stripped one: {labels:?}",
                glyphs.name
            );
            assert!(
                labels.contains(&"rust-review__no-secrets"),
                "{}: {labels:?}",
                glyphs.name
            );
            assert!(
                !labels.contains(&"mcp servers"),
                "{}: a group with nothing in it is not drawn: {labels:?}",
                glyphs.name
            );
            assert!(
                rows.iter().any(|row| row.heading),
                "{}: the groups are headings",
                glyphs.name
            );
        }
    }

    /// A hook is named, never described as unnameable.
    ///
    /// Sabotage: draw the group off `contributions` again. The row's label is then
    /// the word `declared` rather than the event, and its detail is a sentence
    /// about io-cli rather than the argv, so both assertions fail.
    #[test]
    fn a_hook_is_drawn_as_its_event_and_its_command() {
        let rows = detail(&listed(), 120, &UNICODE);
        let position = rows
            .iter()
            .position(|row| row.label == "hooks")
            .expect("a bundle declaring hooks says so");
        assert!(rows[position].heading);
        assert_eq!(rows[position + 1].label, "tool_call");
        assert_eq!(
            rows[position + 1].detail.as_deref(),
            Some("[cargo fmt]"),
            "the argv is what the row is for",
        );
        assert!(
            !rows.iter().any(|row| row
                .detail
                .as_deref()
                .is_some_and(|said| said.contains("io-harness does not expose"))),
            "the placeholder outlived the accessor that replaced it: {rows:?}",
        );

        // A bundle with no hooks draws no group, so the heading is never over an
        // empty list. `contributions` is deliberately left saying `hooks`: the
        // rows come off the hooks themselves now, and nothing keys on the word.
        let mut quiet = listed();
        quiet.hooks = Vec::new();
        assert!(detail(&quiet, 120, &UNICODE)
            .iter()
            .all(|row| row.label != "hooks"));
    }

    #[test]
    fn a_bundle_with_nothing_listable_still_draws_a_row() {
        let plugin = Listed {
            id: "bare".to_string(),
            enabled: true,
            description: None,
            version: None,
            root: PathBuf::from("/repo/bare"),
            contributions: Vec::new(),
            skills: None,
            templates: None,
            agents: Vec::new(),
            servers: Vec::new(),
            hooks: Vec::new(),
            layers: Vec::new(),
        };
        let rows = detail(&plugin, 80, &ASCII);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "this bundle contributes nothing");
    }

    #[test]
    fn the_edits_name_the_plugin_array() {
        assert_eq!(
            add(Path::new("bundles/rust-review")),
            crate::edit::Edit::append("plugin", "path = \"bundles/rust-review\"")
        );
        assert_eq!(
            remove(2),
            crate::edit::Edit::remove("plugin[2]".to_string())
        );
    }

    /// An install writes **one** entry, switched on, and the toggle is a `set` on
    /// that entry's own key rather than a second entry.
    ///
    /// The 0.29.0 form of this test asserted a `path = …\nenabled = false` append,
    /// because the install had to declare a bundle off in order to have io-harness
    /// read it at all. `Plugins::inspect` removed that round trip, so an install is
    /// one plain `add` and the `enabled` key appears only when somebody switches a
    /// bundle off on purpose.
    #[test]
    fn an_install_declares_one_entry_and_the_toggle_is_one_key() {
        assert_eq!(
            add(Path::new("bundles/rust-review")),
            crate::edit::Edit::append("plugin", "path = \"bundles/rust-review\"")
        );
        assert_eq!(
            enable(3),
            crate::edit::Edit::set("plugin[3].enabled", "true")
        );
        assert_eq!(
            disable(3),
            crate::edit::Edit::set("plugin[3].enabled", "false")
        );
    }

    #[test]
    fn a_windows_path_is_escaped_rather_than_handed_over_as_escapes() {
        assert_eq!(
            add(Path::new(r"C:\bundles\rust-review")),
            crate::edit::Edit::append("plugin", r#"path = "C:\\bundles\\rust-review""#)
        );
    }

    /// **What `quoted` wrote is what the readers read back, escape for escape.**
    ///
    /// Both readers took the value's bytes and stripped its quotes with
    /// `trim_matches('"')` until 0.29.0, which undoes nothing — so every path with
    /// a `\` or a `"` in it came back doubled, and `declared_off` handed a path
    /// like that to the manifest read that decides what an install discloses.
    ///
    /// Sabotage: put `raw.trim().trim_matches('"')` back in either reader. The
    /// round trip below fails on the first two cases — and the second is the one
    /// `trim_matches` mangles twice over, since it takes the escaped quote's
    /// backslash off the end along with the closing quote.
    #[test]
    fn a_path_survives_being_written_and_read_back() {
        for text in [
            r"/Users/someone/plugins/a\b",
            "/Users/someone/plugins/a\"b",
            r"C:\bundles\rust-review",
            "bundles/rust-review",
            "",
        ] {
            assert_eq!(
                unquoted(&quoted(text)),
                text,
                "the write and the read disagree about {text:?}, so a bundle is \
                 declared at one path and looked for at another",
            );
        }

        // And through the reader an install actually uses, which is where the
        // wrong path becomes a different manifest. `declared_at` rather than the
        // deleted `declared_off`: the round trip being asserted is the same one —
        // write a path, find that entry again by it — and it is now the only reader
        // that makes it.
        let written =
            crate::edit::apply("", &[add(Path::new(r"plugins/a\b"))]).expect("the entry applies");
        assert_eq!(
            crate::edit::value_at(&written, "plugin[0].path").as_deref(),
            Some(quoted(r"plugins/a\b").as_str()),
            "the entry that was just written cannot be read back: {written}",
        );
        assert_eq!(
            unquoted(&crate::edit::value_at(&written, "plugin[0].path").unwrap_or_default()),
            r"plugins/a\b",
            "and decoding it does not give back the path it was written from",
        );
    }

    #[test]
    fn the_view_reads_the_loaded_and_the_refused_out_of_one_configuration() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = dir.path();
        std::fs::create_dir_all(root.join("good").join("skills")).expect("the bundle directory");
        std::fs::write(
            root.join("good").join("plugin.toml"),
            "name = \"rust-review\"\ndescription = \"reviews\"\nversion = \"1.2.0\"\n\
             skills = \"skills\"\n\n[[agent]]\nname = \"reviewer\"\n",
        )
        .expect("the manifest");
        std::fs::create_dir(root.join("bad")).expect("the empty bundle directory");
        std::fs::write(
            root.join("io.local.toml"),
            "[[plugin]]\npath = \"good\"\n\n[[plugin]]\npath = \"bad\"\n",
        )
        .expect("the configuration");

        let config = io_harness::Config::discover(root).expect("the configuration loads");
        let view = view(&config);
        assert!(!view.is_empty());

        let plugin = view
            .plugins
            .iter()
            .find(|plugin| plugin.id == "rust-review")
            .expect("the good bundle loaded");
        assert_eq!(plugin.description.as_deref(), Some("reviews"));
        assert_eq!(plugin.version.as_deref(), Some("1.2.0"));
        assert_eq!(plugin.contributions, vec!["skills", "agents"]);
        assert_eq!(
            plugin.agents,
            vec!["rust-review__reviewer".to_string()],
            "the name is namespaced by io-harness before io-cli ever sees it",
        );

        let refused = view
            .refused
            .iter()
            .find(|refused| refused.path.ends_with("bad"))
            .expect("the manifest-less bundle was dropped rather than erroring");
        assert!(
            refused.error.contains("plugin.toml"),
            "the harness's own sentence, whole: {}",
            refused.error
        );
    }
}
