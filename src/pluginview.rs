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
//! # Hooks cannot be listed, and this file says so on screen
//!
//! There is no `Plugin::hooks()` and `Hook` is `pub(crate)`. A bundle's hooks are
//! applied — [`io_harness::Plugins::apply_to_hooks`] installs them — and there is
//! no API by which io-cli can name one, count them, or say what any of them runs.
//! The string `"hooks"` from [`io_harness::Plugin::contributions`] is the entire
//! honest signal, and [`detail`] draws exactly that: a row saying hooks were
//! declared and that io-cli cannot say what they do. **The alternative was to omit
//! the row**, which reads as a bundle with no hooks — the one reading that is
//! actually false, on the contribution kind that runs programs.
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
//! manifest means, and `tests/dependencies.rs` forbids one by path. Like
//! [`crate::servers`] and [`crate::skillview`], this is a data model and pure
//! functions: the driver in `src/main.rs` owns the keyboard and applies what
//! [`add`] and [`remove`] return.

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

/// The narrowest a bundle's root may be drawn at, before its separator.
///
/// Twenty cells is `...bundles/rust-review` — the last segments and the mark
/// saying the front went, which is what identifies a directory on a machine where
/// every bundle shares the first several segments of its path. The same floor
/// [`crate::skillview`] fits a skill file to, and for the same reason: a row may
/// lose a fact, and may not draw one that cannot be read.
const ROOT_FLOOR: usize = 20;

/// One bundle that loaded, as `/plugin` lists it.
///
/// Everything is owned and copied out of the borrowed [`io_harness::Plugin`],
/// because `Config::plugins()` returns a fresh value that re-read the disk: the
/// borrow dies with the call, and a view that held it would tie every surface to
/// the lifetime of one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The manifest's `name`, which is also what every contribution below is
    /// namespaced by.
    pub id: String,
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
    ///
    /// **This is the only place `hooks` can come from.** See the module docs.
    pub contributions: Vec<&'static str>,
    /// The skills directory it contributes, absolute, if it declared one.
    pub skills: Option<PathBuf>,
    /// The templates directory it contributes, absolute, if it declared one.
    pub templates: Option<PathBuf>,
    /// The agent names it contributes, already namespaced.
    pub agents: Vec<String>,
    /// The MCP server ids it contributes, already namespaced.
    pub servers: Vec<String>,
    /// The policy layer names it contributes, already namespaced. Deny rules
    /// only — io-harness drops a bundle whose layer carries anything else.
    pub layers: Vec<String>,
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
    /// The bundles that loaded, in the order they were declared — which is the
    /// order their policy layers stack and their contributions are applied in.
    pub plugins: Vec<Listed>,
    /// The bundles that did not, with the reason each one did not.
    pub refused: Vec<Refused>,
}

impl View {
    /// Whether the configuration declared no bundle at all.
    ///
    /// **Both lists, because a configuration whose only bundle was refused has
    /// declared one.** `Plugins::is_empty` says nothing about `dropped()`, and a
    /// surface that read it alone would tell an operator with a broken manifest
    /// that they have no plugins — which is the false sentence this module exists
    /// to stop being told.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty() && self.refused.is_empty()
    }
}

/// Every bundle this configuration declared, loaded and refused.
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
        plugins: plugins
            .iter()
            .map(|plugin| Listed {
                id: plugin.id().to_string(),
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
                layers: plugin
                    .policy_layers()
                    .iter()
                    .map(|layer| layer.name.clone())
                    .collect(),
            })
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
    }
}

/// The picker rows for the whole list, fitted for a terminal this wide.
///
/// **One row per bundle, loaded first and refused after, and no headings.** The
/// picker hands a chosen index straight back into the caller's own list, so a
/// heading row is an index that maps to no bundle — [`crate::commands`] carries a
/// parallel vector of `Held::Nothing` to survive that, and this surface has two
/// lists to index rather than one. So the contract is positional and written down:
/// **index `i` is `view.plugins[i]` while `i < view.plugins.len()`, and
/// `view.refused[i - view.plugins.len()]` after it.** The mark, not a heading,
/// says which — and unlike a heading it survives a typed query, which is exactly
/// when a refused bundle is hardest to tell from a loaded one.
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

    for plugin in &view.plugins {
        // Commas inside a field, the glyph set's separator between fields: this
        // is one fact — the list of kinds — and punctuating it with the same run
        // that divides the row would read as five fields.
        let mut detail = plugin.contributions.join(", ");
        if detail.is_empty() {
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

        if let Some(description) = &plugin.description {
            if left > floor {
                let described = fit(description, left - separator_width, glyphs);
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

        out.push(Row::marked(LOADED_MARK, plugin.id.clone(), detail));
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
    if plugin.contributions.contains(&"hooks") {
        out.push(Row::heading("hooks"));
        // **The whole of what io-cli knows, stated as the whole of what it
        // knows.** There is no accessor and `Hook` is `pub(crate)`, so the count,
        // the events and the argv are all unreachable — and a bundle's hooks are
        // the contribution that runs programs. Omitting the group would read as a
        // bundle with no hooks, which is the one reading that is false.
        out.push(Row::with_detail(
            "declared",
            fit(
                // No dash and no other mark that would need a glyph set: this is a
                // rendered string, and the one class of mark in it — the
                // separator between the two clauses — is a semicolon in every
                // terminal there is.
                "io-harness does not expose a bundle's hooks, so io-cli cannot say what they run; \
                 read the bundle's plugin.toml",
                room.saturating_sub("declared".chars().count() + 2),
                glyphs,
            ),
        ));
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
            let declared = PathBuf::from(raw.trim().trim_matches('"'));
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
fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::{ASCII, UNICODE};

    /// A bundle with something of every kind that can be listed.
    fn listed() -> Listed {
        Listed {
            id: "rust-review".to_string(),
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
            layers: vec!["rust-review__no-secrets".to_string()],
        }
    }

    fn view_of(plugin: Listed) -> View {
        View {
            plugins: vec![plugin],
            refused: Vec::new(),
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

    #[test]
    fn hooks_are_declared_as_unlistable_rather_than_omitted() {
        let rows = detail(&listed(), 120, &UNICODE);
        let position = rows
            .iter()
            .position(|row| row.label == "hooks")
            .expect("a bundle declaring hooks says so");
        assert!(rows[position].heading);
        let said = rows[position + 1]
            .detail
            .clone()
            .expect("the hooks row explains itself");
        assert!(
            said.contains("io-harness does not expose"),
            "io-cli says what it cannot say: {said}"
        );

        // And a bundle with no hooks has no such group, so the sentence above is
        // never drawn over a bundle it is not true of.
        let mut quiet = listed();
        quiet.contributions = vec!["skills"];
        assert!(detail(&quiet, 120, &UNICODE)
            .iter()
            .all(|row| row.label != "hooks"));
    }

    #[test]
    fn a_bundle_with_nothing_listable_still_draws_a_row() {
        let plugin = Listed {
            id: "bare".to_string(),
            description: None,
            version: None,
            root: PathBuf::from("/repo/bare"),
            contributions: Vec::new(),
            skills: None,
            templates: None,
            agents: Vec::new(),
            servers: Vec::new(),
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

    #[test]
    fn a_windows_path_is_escaped_rather_than_handed_over_as_escapes() {
        assert_eq!(
            add(Path::new(r"C:\bundles\rust-review")),
            crate::edit::Edit::append("plugin", r#"path = "C:\\bundles\\rust-review""#)
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
