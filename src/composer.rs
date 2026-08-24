//! The composer: the one part of the viewport the user types into.
//!
//! `tui-textarea` does the editing. What is added here is the small set of
//! decisions a prompt has that a text editor does not: when `Enter` submits and
//! when it inserts a newline, what the arrow keys mean at the edges of the text,
//! and how a paste of a whole file behaves.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::Frame;
use tui_textarea::{CursorMove, TextArea};

use crate::theme::{Theme, Tone};

/// The prompt marker. Two cells, so wrapped continuation lines line up under the
/// text rather than under the marker.
pub const PROMPT: &str = "> ";

/// How many characters a paste may carry before it is collapsed to one line.
///
/// Two rows of an eighty-column terminal, less the two the prompt marker takes:
/// `2 × 78`. The number is the composer's own size rather than a round one —
/// `crate::app` gives the composer exactly two of the four viewport rows, so a
/// paste of a hundred and fifty-six characters is the largest one the operator
/// can still see all of. The first character past that is the first one that
/// falls outside a box two rows tall, and from there the input stops being
/// navigable in the only sense that matters to somebody about to send it: what
/// is on screen is no longer what is in the prompt.
///
/// It is a character count and not a line count on purpose. A pasted block of
/// three short lines is three rows and still perfectly readable, and collapsing
/// it would take away the one thing a bracketed paste is for.
pub const PASTE_THRESHOLD: usize = 156;

/// What a keystroke did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Handled; nothing for the session to do.
    Idle,
    /// The user pressed `Enter` on a finished prompt.
    Submitted(String),
}

/// The path a paste names, if it names one that exists.
///
/// Dragging a file into a terminal pastes its path, and a file manager's copy
/// does the same. What arrives may be shell-escaped — `My\ Documents` — or
/// already quoted, and it is one line either way. Quoting it is what makes a
/// path with a space in it survive as one word; resolving it to an absolute path
/// is what makes it survive the agent's working directory being somewhere else.
///
/// **It has to exist.** The whole safety of this is that prose is never quoted
/// at somebody: a sentence is not a path, and a path this process cannot see is
/// not one either, so both are pasted exactly as they arrived.
/// `text` with every `%XX` turned back into the byte it stands for.
///
/// Only what a `file://` URL needs. A sequence that is not valid UTF-8 once
/// decoded is handed back as it came, because a path this cannot read is not a
/// path this should guess at.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[at + 1..at + 3], 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// Every path a paste names, when it names paths at all.
///
/// **A drop of three files is one paste**, and the terminal writes the paths on
/// one line separated by spaces, with any space inside a name escaped — or, in
/// some terminals, one per line. Read as a single string none of that is a path,
/// so a multiple selection dropped on the prompt did nothing at all: it fell
/// through to being pasted as text.
///
/// The whole text is tried first, so a name with an unescaped space in it — which
/// is what a copied path from a file manager looks like — is still one path. Then
/// lines, then space-separated tokens with `\ ` treated as an escape. A split is
/// only accepted when *every* piece of it names something that exists, which is
/// what keeps a sentence about two files from being read as two files.
pub fn pasted_paths(text: &str) -> Vec<String> {
    if let Some(one) = pasted_path(text) {
        return vec![one];
    }
    let lines = split_lines(text);
    if lines.len() > 1 {
        let found: Vec<String> = lines.iter().filter_map(|line| pasted_path(line)).collect();
        if found.len() == lines.len() {
            return found;
        }
    }
    split_greedy(text)
}

fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// How many whitespace-separated pieces a paste may have before this stops
/// trying to read it as a list of paths.
///
/// The scan below asks the filesystem about a prefix at a time, so a paragraph
/// would be a great many questions to answer "no, that is prose". A drop or a
/// copy of files is a handful of paths; anything past this is not one.
const MOST_PIECES: usize = 64;

/// `text` read as a sequence of paths, longest match first.
///
/// **This is the one that reads what `Cmd+C` in Finder writes.** A drop escapes
/// the spaces inside a name; a copy does not — so `…/Screenshot 2026-08-24 at
/// 8.52.27 PM.png` arrives as eight words, and two of those pasted together are
/// sixteen with no marker anywhere saying where one path ends and the next
/// begins. Splitting on spaces finds nothing that exists; splitting on nothing
/// finds one thing that does not.
///
/// So the filesystem is what says where the boundary is: take the longest run of
/// words from here that names a file, keep it, and start again after it. A run
/// that names nothing ends the whole attempt, which is what stops a sentence
/// mentioning a file from being read as one.
fn split_greedy(text: &str) -> Vec<String> {
    // Where each word starts and ends, as byte offsets into `text`. Offsets and
    // not the words themselves, because a candidate has to be a *slice of the
    // original*: joining words back together with a space is what broke this the
    // first time. macOS writes a narrow no-break space — U+202F — before the `AM`
    // in every screenshot's name, and that character is whitespace to
    // `split_whitespace` and not a space to the filesystem, so every rejoined
    // candidate named a file that does not exist.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (at, character) in text.char_indices() {
        match (character.is_whitespace(), start) {
            (false, None) => start = Some(at),
            (true, Some(from)) => {
                words.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push((from, text.len()));
    }
    if words.len() < 2 || words.len() > MOST_PIECES {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut at = 0;
    while at < words.len() {
        let mut took = None;
        for end in (at + 1..=words.len()).rev() {
            if let Some(path) = pasted_path(&text[words[at].0..words[end - 1].1]) {
                took = Some((path, end));
                break;
            }
        }
        match took {
            Some((path, end)) => {
                found.push(path);
                at = end;
            }
            None => return Vec::new(),
        }
    }
    found
}

pub fn pasted_path(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }
    // A `file://` URL is what several applications put on the pasteboard instead
    // of a path, and what it carries is percent-encoded — `%20` for every space
    // in a screenshot's name.
    let unescaped = trimmed.trim_matches(['"', '\'']).replace("\\ ", " ");
    let unescaped = match unescaped.strip_prefix("file://") {
        // `file:///C:/Users/…` on Windows: the authority is empty and the path
        // that follows opens with a slash the drive letter does not want.
        Some(rest) => {
            let decoded = percent_decoded(rest);
            match decoded.strip_prefix('/') {
                Some(drive) if drive.chars().nth(1) == Some(':') => drive.to_string(),
                _ => decoded,
            }
        }
        None => unescaped,
    };
    let candidate = std::path::Path::new(&unescaped);
    if !candidate.exists() {
        return None;
    }
    Some(
        candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf())
            .display()
            .to_string(),
    )
}

/// `path` wrapped in quotes, and nothing else done to it.
///
/// **A quoting, never a debug escape, and 0.13.1 is what that cost.** This wrote
/// `format!("{path:?}")` up to 0.13.0, and `Debug` for a string escapes every
/// character Rust considers unprintable — which includes the U+202F narrow
/// no-break space macOS puts between the time and the `AM` in every screenshot's
/// name. What landed on the prompt line was `\u{202f}` as six literal characters,
/// so the path named no file even once its quotes came off, and `/attach` refused
/// the operator's own screenshot in a sentence about image formats.
///
/// The mark is chosen rather than escaped into: a path carrying a double quote is
/// wrapped in single quotes, one carrying both is left bare. That keeps this the
/// exact inverse of [`crate::attach::unquote`], which takes off one matching pair
/// and knows nothing about escapes — two halves that agree because neither of
/// them has anything to agree about.
fn quoted(path: &str) -> String {
    match (path.contains('"'), path.contains('\'')) {
        (false, _) => format!("\"{path}\""),
        (true, false) => format!("'{path}'"),
        (true, true) => path.to_string(),
    }
}

pub struct Composer {
    area: TextArea<'static>,
    /// Prompts already submitted, oldest first.
    history: Vec<String>,
    /// Where the arrow keys currently are in `history`, or `None` when the user
    /// is editing their own text rather than walking back through old prompts.
    recalled: Option<usize>,
    /// What was in the composer when the walk back started, so `Down` past the
    /// newest entry returns it rather than clearing it.
    draft: String,
    /// Every block that was pasted in and shown as one line, as
    /// `(placeholder, what it stands for)`, oldest first.
    ///
    /// The composer owns them, and [`Composer::text`] — not the submit path —
    /// puts them back. That is the whole design decision: a placeholder that is
    /// expanded only on `Enter` is one caller away from sending a description of
    /// a file where the file should have been, and nothing on screen would say
    /// so. Here the collapsed form exists only inside this type.
    pastes: Vec<(String, String)>,
    /// Markers standing for an attached image, oldest first.
    ///
    /// Unlike a paste's placeholder these expand to nothing: `[Image #1]` is
    /// what the operator sees, what the agent is told, and what the transcript
    /// records — the picture itself rides the turn as media rather than as text.
    /// They are here for one reason: so that a marker deletes as one thing, the
    /// way a pasted block does, instead of leaving `[Image #` on a prompt.
    markers: Vec<(String, String)>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    pub fn new() -> Self {
        let mut area = TextArea::default();
        // The composer draws its own prompt marker, so the widget must not also
        // draw a border, a line number or a cursor-line highlight.
        area.set_cursor_line_style(ratatui::style::Style::default());
        Self {
            area,
            history: Vec::new(),
            recalled: None,
            draft: String::new(),
            pastes: Vec::new(),
            markers: Vec::new(),
        }
    }

    /// What is currently typed, with every collapsed paste put back whole.
    ///
    /// This is the prompt, not the picture of it: a paste over
    /// [`PASTE_THRESHOLD`] is one line on screen and its full text here, byte
    /// for byte. Callers get the text they would have got had the paste been
    /// inserted as it always was.
    pub fn text(&self) -> String {
        self.expand(&self.typed())
    }

    /// What is on screen, placeholders and all.
    ///
    /// **What is on screen, and never what will be sent.** Everything that acts
    /// on the prompt asks [`Composer::text`], which cannot forget a paste; this
    /// is for the surfaces and the tests that need to know what the operator is
    /// actually looking at — a placeholder standing for a block, rather than the
    /// block.
    pub fn typed(&self) -> String {
        self.area.lines().join("\n")
    }

    /// `typed` with each placeholder replaced by the block it stands for.
    ///
    /// One left-to-right pass, never re-scanning what it has already written, so
    /// a pasted block that happens to contain the literal text of a placeholder
    /// is copied out rather than expanded a second time. A chain of
    /// `str::replace` calls would have expanded it, and pasting a transcript of a
    /// session that already collapsed a paste is all it would take.
    ///
    /// A placeholder is matched by its exact text. Edit a character of one and it
    /// stops being a placeholder and is sent as the words it reads as: the
    /// composer has no way to tell a person rewriting the line from a person
    /// typing that sentence, and quietly attaching a file to a line somebody
    /// edited is the worse of the two mistakes.
    fn expand(&self, typed: &str) -> String {
        if self.pastes.is_empty() {
            return typed.to_string();
        }
        let mut out = String::with_capacity(typed.len());
        let mut rest = typed;
        'text: while !rest.is_empty() {
            for (placeholder, text) in &self.pastes {
                if let Some(tail) = rest.strip_prefix(placeholder.as_str()) {
                    out.push_str(text);
                    rest = tail;
                    continue 'text;
                }
            }
            let mut characters = rest.chars();
            out.push(characters.next().expect("the remainder is not empty"));
            rest = characters.as_str();
        }
        out
    }

    /// Whether there is nothing here worth sending or worth keeping. `Ctrl+D`
    /// exits, `Esc` arms the rewind and `/` opens the palette on an empty
    /// composer, so this decides what three keystrokes mean.
    ///
    /// Asked of [`Composer::text`] rather than of the widget's lines, because
    /// those two disagree the moment a paste is not what it looks like. Paste a
    /// blank region of a file — a run of indentation, a column of empty lines —
    /// and it is over [`PASTE_THRESHOLD`], so it collapses to a placeholder that
    /// is emphatically not whitespace, while the prompt it stands for is nothing
    /// at all. Read the screen and the composer looks full; `Enter` reads the
    /// expanded text and will not send it. The operator is then holding a prompt
    /// that cannot be submitted, cannot be exited and cannot be cleared, and
    /// nothing on the frame says why.
    ///
    /// So there is one question and `Enter` is where it is answered: what the
    /// callers gate on is exactly what would be sent. The cost is expanding the
    /// pastes for a keystroke that only wanted to know whether the prompt is
    /// blank, which is bounded by what a terminal hands over in one paste event.
    /// A cheaper second predicate that could drift from the first is precisely
    /// what this was.
    pub fn is_empty(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// The prompts submitted so far, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Rows the composer needs at this width, prompt marker included.
    pub fn height(&self, width: u16) -> u16 {
        let room = usize::from(width.saturating_sub(PROMPT.len() as u16)).max(1);
        u16::try_from(self.wrapped(room).0.len()).unwrap_or(u16::MAX).max(1)
    }

    /// The prompt as the rows it actually occupies at `room` columns, and where
    /// the insertion point is among them as `(row, column)`.
    ///
    /// **One function, because there used to be three answers and they
    /// disagreed.** `height`, `rows_wanted` and `cursor` each did this
    /// arithmetic, and the widget underneath did something else again: a
    /// `TextArea` does not wrap, it scrolls sideways. So a prompt long enough to
    /// wrap was drawn clipped at the left while io-cli had grown the viewport for
    /// rows the widget was never going to use, and the cursor io-cli placed and
    /// the block the widget drew sat in two different places — the two cursors
    /// 0.13.1 was reported with. Everything visual now comes from here.
    ///
    /// A logical line that is exactly `room` wide takes one row, and the cursor
    /// past its last character is on the next one. That is what an editor does,
    /// and it is why the cursor's row can be one past the text's.
    fn wrapped(&self, room: usize) -> (Vec<String>, (usize, usize)) {
        let (at_row, at_column) = self.area.cursor();
        let mut rows: Vec<String> = Vec::new();
        let mut at = (0, 0);
        for (number, line) in self.area.lines().iter().enumerate() {
            let first = rows.len();
            let characters: Vec<char> = line.chars().collect();
            if characters.is_empty() {
                rows.push(String::new());
            } else {
                for chunk in characters.chunks(room) {
                    rows.push(chunk.iter().collect());
                }
            }
            if number == at_row {
                at = (first + at_column / room, at_column % room);
                // The insertion point past the end of a row that is exactly full
                // stays *on* that row, at the edge, rather than opening one
                // below it. A terminal's own cursor does the same thing — it
                // rests in the last column with the wrap pending — and the
                // alternative is a composer that grows a row for a prompt whose
                // text ends flush, which is what `f9_a_prompt_wider_than_the_
                // terminal_asks_for_more_rows` has asserted since 0.11.0.
                if at.0 >= rows.len() {
                    at = (rows.len().saturating_sub(1), room);
                }
            }
        }
        if rows.is_empty() {
            rows.push(String::new());
        }
        (rows, at)
    }

    /// The first visible row, given `height` rows to draw in.
    ///
    /// The window follows the insertion point rather than the end of the text, so
    /// editing the middle of a long prompt keeps what is being edited on screen.
    fn scroll(rows: usize, at: usize, height: usize) -> usize {
        if height == 0 || rows <= height {
            return 0;
        }
        at.saturating_sub(height - 1).min(rows - height)
    }

    /// Feed a key.
    pub fn key(&mut self, key: KeyEvent) -> Reply {
        match (key.code, key.modifiers) {
            // `Shift+Enter` is the newline every terminal that can report it uses.
            // Reachable because `term::negotiate_keyboard` asks for
            // `DISAMBIGUATE_ESCAPE_CODES` on the terminals that advertise it:
            // without that flag no terminal reports a modifier on `Enter` at all,
            // and this arm is a binding nobody can reach.
            // **Three ways to the same newline, because one of them is not
            // deliverable.** `Shift+Enter` needs the Kitty keyboard protocol:
            // without it a terminal sends the identical byte for `Enter` and
            // `Shift+Enter`, so the binding is unreachable and the key reads as
            // broken. `Alt+Enter` is reported by terminals that report no
            // modifier on `Enter` at all, and `Ctrl+J` is a byte — `0x0a` — that
            // every terminal on earth sends. The trailing backslash below is the
            // fourth, for the ones that manage none of these.
            (KeyCode::Enter, m)
                if m.contains(KeyModifiers::SHIFT) || m.contains(KeyModifiers::ALT) =>
            {
                self.editing();
                self.area.insert_newline();
                Reply::Idle
            }
            (KeyCode::Char('j'), m) if m == KeyModifiers::CONTROL => {
                self.editing();
                self.area.insert_newline();
                Reply::Idle
            }
            (KeyCode::Enter, _) => {
                // The fallback for terminals that cannot distinguish `Shift+Enter`
                // from `Enter` at all — which is most of them without the Kitty
                // keyboard protocol. A trailing backslash means "continue".
                //
                // It stays even though the arm above now works: the protocol is
                // negotiated only where the terminal advertises it, and the
                // terminal on the far end of an ssh session, a tmux without
                // `extended-keys`, or a plain xterm advertises nothing. Deleting a
                // fallback because its replacement works on the developer's
                // terminal is how somebody else loses the newline entirely.
                if self.current_line().ends_with('\\') {
                    self.editing();
                    self.area.delete_char();
                    self.area.insert_newline();
                    return Reply::Idle;
                }
                let text = self.text();
                // The same test [`Composer::is_empty`] makes, on the same
                // expanded text — it is written out here only because the text
                // is already in hand. Change one and change the other, or the
                // composer goes back to refusing to send a prompt it also
                // refuses to call empty.
                if text.trim().is_empty() {
                    return Reply::Idle;
                }
                self.clear();
                self.remember(&text);
                Reply::Submitted(text)
            }
            // **A placeholder deletes as one thing, because it is one thing.**
            // Thirty-five presses to remove `[pasted text #4, 366 characters]`
            // is bad enough; the first of them is worse, because a placeholder
            // is matched by its exact text and an edited one silently stops
            // standing for the block it named.
            // **Every backwards deletion, not just the plain one.** This used to
            // exclude `Alt`, so `Option+Backspace` — the delete-word every macOS
            // reader has in their fingers — fell through to the widget and ate
            // `[pasted text #8, 464 chara` one word at a time, leaving a
            // placeholder that had silently stopped standing for anything.
            // `Ctrl+W` is the same key by another name.
            (KeyCode::Backspace, _) => self.delete_backwards(key),
            (KeyCode::Char('w'), m) if m == KeyModifiers::CONTROL => {
                self.delete_backwards(key)
            }
            // History, but only from the edge of the text: inside a multiline
            // prompt the arrows have to move the cursor, or a long prompt cannot
            // be edited at all.
            (KeyCode::Up, _) if self.at_first_line() => {
                self.recall_older();
                Reply::Idle
            }
            (KeyCode::Down, _) if self.at_last_line() => {
                self.recall_newer();
                Reply::Idle
            }
            _ => {
                self.editing();
                self.area.input(key);
                Reply::Idle
            }
        }
    }

    /// Take a bracketed paste.
    ///
    /// Inserted verbatim, newlines included, rather than replayed as keystrokes —
    /// which is what an unbracketed paste of a multi-line block does, and it
    /// submits the prompt on the first newline.
    ///
    /// A paste over [`PASTE_THRESHOLD`] leaves one line saying what it is and how
    /// large, and the block itself is kept beside the text and put back by
    /// [`Composer::text`]. Anything at or under the threshold is inserted exactly
    /// as it always was.
    pub fn paste(&mut self, text: &str) {
        self.editing();

        // **A path pasted is a path, and it is quoted.** Dragging a file into a
        // terminal, or copying one out of a file manager, pastes its path — and a
        // path with a space in it is two words to everything downstream unless
        // something quotes it. The check is that it names a file that exists, so
        // ordinary prose is never quoted at somebody.
        if let Some(path) = pasted_path(text) {
            self.area.insert_str(quoted(&path));
            return;
        }

        // **The same block pasted again toggles what is on screen.** The first
        // paste collapses to a placeholder because a screenful of someone else's
        // text is not a prompt you can read; pressing paste again on the same
        // block is the operator saying they want to see it, and pressing it once
        // more is them saying they have seen enough.
        //
        // **It toggles both ways since 0.13.1**, and the missing half was a
        // defect an operator hit in the first minute: expanding used to leave the
        // block in the prompt with its placeholder gone, so the *next* paste of
        // the same clipboard matched nothing and appended a fresh placeholder —
        // `[pasted text #2, 462 characters]`, then `#3`, then `#4`, piling up
        // after text that was already there. The block is looked up by what it
        // holds, and what is on screen decides which way the toggle goes.
        if let Some((placeholder, held)) =
            self.pastes.iter().find(|(_, held)| held == text).cloned()
        {
            let typed = self.typed();
            if typed.contains(placeholder.as_str()) {
                let expanded = typed.replace(&placeholder, &held);
                self.replace(&expanded);
                return;
            }
            if typed.contains(held.as_str()) {
                let collapsed = typed.replace(&held, &placeholder);
                self.replace(&collapsed);
                return;
            }
            // **Neither form is in the prompt, so this is an insertion — but of a
            // block this composer already knows, under the number it already
            // has.** The operator expanded the paste and then edited it, which is
            // the ordinary thing to do, and the edit means the block is no longer
            // in the prompt verbatim. Minting `#2`, then `#3`, then `#4` for the
            // same clipboard is what an operator was shown, and none of them
            // could be toggled either, because each new placeholder stood for a
            // block whose expanded form was already there under somebody's edits.
            //
            // One number per block, for the life of the prompt. The next press
            // finds this placeholder on screen and expands it, so the toggle is
            // working again on the very next keystroke rather than never.
            self.area.insert_str(&placeholder);
            return;
        }

        // `chars`, never `len`: a byte count is not the size the operator can
        // check against what they copied, and it is off by a factor of three for
        // a paste of prose in most of the world's scripts.
        let size = text.chars().count();
        if size <= PASTE_THRESHOLD {
            self.area.insert_str(text);
            return;
        }
        // Content before metadata: what this line is, then how large it is. No
        // mark from `theme.glyphs` appears in it, deliberately — the line is
        // composed here, where the composer has no theme in hand, and a glyph set
        // stashed on this type would be a second one derived downstream of the
        // one chosen at startup, which is the thing `crate::glyphs` says nothing
        // may do. Every character below is in both sets, so the line reads the
        // same under either.
        //
        // The ordinal is what keeps two placeholders apart: two pastes of the
        // same size would otherwise spell the same line, and the second would be
        // sent as the first. It counts every paste this prompt has taken, not the
        // ones still in the text, so deleting one cannot free its number for
        // reuse.
        let placeholder = format!(
            "[pasted text #{}, {size} characters]",
            self.pastes.len() + 1
        );
        self.area.insert_str(&placeholder);
        self.pastes.push((placeholder, text.to_string()));
    }

    /// Put a marker for an attached image in the prompt.
    ///
    /// **The marker is the whole of what is sent about the picture.** It goes to
    /// the model as the words `[Image #1]`, and the picture itself rides the turn
    /// as media, staged on the session — so the prompt the operator reads, the
    /// prompt the agent is given and the row the transcript keeps are the same
    /// three characters of text. Nothing is drawn: an image in a terminal is
    /// twenty rows of somebody's screenshot in the middle of a conversation, and
    /// `/image` is there for the moment somebody wants to see it again.
    ///
    /// It deletes as one thing, exactly as a pasted block does.
    pub fn attach(&mut self, marker: &str, path: &str) {
        self.editing();
        // **Pasting the same picture again toggles what is on the prompt**, the
        // way pasting the same block of text does: the marker is what it reads as
        // by default, and the path is there for an operator checking they
        // attached the right file. Either way the picture itself is staged on the
        // turn — this is a view of an attachment, not the attachment.
        let typed = self.typed();
        if let Some((held, held_path)) = self
            .markers
            .iter()
            .find(|(_, held_path)| held_path == path)
            .cloned()
        {
            // Quoted, for the same reason any pasted path is: a path with a
            // space in it is two words to everything downstream unless something
            // says otherwise, and the operator toggling to the path is usually
            // checking they attached the file they meant.
            let shown_path = quoted(&held_path);
            if typed.contains(held.as_str()) {
                let shown = typed.replace(&held, &shown_path);
                self.replace(&shown);
                return;
            }
            if typed.contains(shown_path.as_str()) {
                let shown = typed.replace(&shown_path, &held);
                self.replace(&shown);
                return;
            }
            // Neither form is on the prompt any more, so this is an insertion of
            // a picture this composer already knows — under the number it already
            // has, never a new one.
            self.area.insert_str(format!("{held} "));
            return;
        }
        // A space after it, so the sentence an operator types next does not run
        // into the bracket.
        self.area.insert_str(format!("{marker} "));
        self.markers.push((marker.to_string(), path.to_string()));
    }

    /// Whether this prompt already stands for `path`.
    ///
    /// Asked by the driver before it stages a picture a second time: a repeat
    /// paste is a request to change what is on screen, not to attach the same
    /// file twice.
    pub fn attached(&self, path: &str) -> bool {
        self.markers.iter().any(|(_, held)| held == path)
    }

    /// Put `text` in the prompt, replacing whatever is there, cursor at the end.
    ///
    /// The slash palette's `Enter`, and its only caller. The command is *typed*
    /// rather than run: the submit path stays the one path — `Enter`, then
    /// `strip_prefix('/')`, then [`crate::commands::parse`] — so the palette adds
    /// no second way for a command to be dispatched, and the operator sees and
    /// can edit what they are about to send.
    ///
    /// It goes through the same replacement a walk back through history uses, so
    /// there is one answer to "what happens to the pasted blocks" and not two.
    /// The palette only opens on an empty prompt, so in practice there is nothing
    /// to carry across.
    pub fn set(&mut self, text: &str) {
        self.replace(text);
    }

    /// Empty the composer, keeping the history.
    ///
    /// The pasted blocks go with the text. They belong to the prompt being
    /// written — once it is submitted or abandoned, a placeholder that survived
    /// would be standing in for something nobody can see any more.
    pub fn clear(&mut self) {
        self.area.select_all();
        self.area.cut();
        self.recalled = None;
        self.draft.clear();
        self.pastes.clear();
        self.markers.clear();
    }

    /// Render into `area`, prompt marker included.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        if area.width <= PROMPT.len() as u16 {
            // Too narrow to draw the prompt and any text beside it — but a frame
            // that accepts input still has to say where the focus is, and a
            // terminal this narrow is the last place to take that away. The
            // caret goes to the origin, which is where the first character
            // would land if there were room for one. Returning here without it
            // was the composer's own version of the defect F2 exists for, and
            // it was the surface that already knew better.
            frame.set_cursor_position(Position {
                x: area.x,
                y: area.y,
            });
            return;
        }
        let marker = Rect {
            width: PROMPT.len() as u16,
            ..area
        };
        let text = Rect {
            x: area.x + PROMPT.len() as u16,
            width: area.width - PROMPT.len() as u16,
            ..area
        };
        frame.render_widget(
            // Bold, because this row is where the operator's attention starts
            // and the mark is the only thing on screen that says "type here".
            ratatui::widgets::Paragraph::new(Line::styled(
                PROMPT,
                theme
                    .style(Tone::Accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
            marker,
        );
        // **The prompt's own rows, wrapped here rather than by the widget.**
        // `tui-textarea` scrolls sideways instead of wrapping, and every other
        // measurement in this product assumes a wrap — so drawing the widget
        // meant a long prompt clipped at the left, a viewport grown for rows
        // nothing used, and the widget's own block cursor sitting somewhere other
        // than the terminal cursor io-cli had placed. The editing is still the
        // widget's; the picture is this crate's.
        let room = usize::from(text.width).max(1);
        let (rows, (at_row, _)) = self.wrapped(room);
        let top = Self::scroll(rows.len(), at_row, usize::from(text.height));
        frame.render_widget(
            ratatui::widgets::Paragraph::new(
                rows.iter()
                    .skip(top)
                    .take(usize::from(text.height))
                    .map(|row| Line::from(row.clone()))
                    .collect::<Vec<_>>(),
            ),
            text,
        );

        // The real terminal cursor is put where the insertion point is, and that
        // is done here rather than left to a caller. ratatui hides the cursor on
        // any frame that does not set a position, and a hidden cursor removes the
        // only focus indicator a screen reader has — the criticism the whole
        // category is unusable for. Owning it in the widget that owns the
        // insertion point means no frame can forget.
        //
        // It is also now the *only* cursor on screen. The widget drew a second
        // one — an inverted cell at its own idea of the insertion point — and the
        // two agreed only while the prompt fitted on one row.
        let (x, y) = self.cursor(text);
        frame.set_cursor_position(Position { x, y });
    }

    /// The cursor's position inside `area`, which is the text region rather than
    /// the whole composer.
    pub fn cursor(&self, text: Rect) -> (u16, u16) {
        // The same wrap the frame is drawn from, and the same scroll, so the
        // caret cannot land on a row the prompt is not on. Reading a second
        // arithmetic here is what put two cursors on the screen.
        let room = usize::from(text.width).max(1);
        let (rows, (at_row, at_column)) = self.wrapped(room);
        let top = Self::scroll(rows.len(), at_row, usize::from(text.height));
        let row = u16::try_from(at_row.saturating_sub(top)).unwrap_or(u16::MAX);
        let column = u16::try_from(at_column).unwrap_or(u16::MAX);
        (
            // Never off the right edge: a caret resting past a full row sits in
            // its last cell, which is where a terminal puts its own.
            (text.x + column).min(text.right().saturating_sub(1)),
            (text.y + row).min(text.bottom().saturating_sub(1)),
        )
    }

    /// How many rows this prompt wants, at `width`.
    ///
    /// What the driver grows the viewport to. Counted rather than guessed: a
    /// line wider than the composer wraps, and a prompt of three wrapped lines
    /// needs the rows those wraps take or the operator is typing into a window
    /// they cannot see the top of.
    pub fn rows_wanted(&self, width: u16) -> u16 {
        self.height(width)
    }

    /// Delete backwards, taking a placeholder as one thing when the cursor is at
    /// the end of one.
    ///
    /// **A placeholder deletes as one thing, because it is one thing.**
    /// Thirty-five presses to remove `[pasted text #4, 366 characters]` is bad
    /// enough; the first of them is worse, because a placeholder is matched by
    /// its exact text and an edited one silently stops standing for the block it
    /// named. Which deletion key was pressed does not change that — a word-wise
    /// delete leaves the same broken half a line as a character-wise one.
    ///
    /// Anything not sitting at the end of a placeholder is the widget's own
    /// deletion, whichever one the key means.
    fn delete_backwards(&mut self, key: KeyEvent) -> Reply {
        self.editing();
        match self.placeholder_before_cursor() {
            Some((placeholder, spaces)) => {
                for _ in 0..placeholder.chars().count() + spaces {
                    self.area.delete_char();
                }
                // The block goes with it. A prompt that still carried it would
                // send text nothing on screen stands for.
                self.pastes.retain(|(held, _)| held != &placeholder);
                self.markers.retain(|(held, _)| held != &placeholder);
            }
            None => {
                self.area.input(key);
            }
        }
        Reply::Idle
    }

    fn current_line(&self) -> &str {
        let (row, _) = self.area.cursor();
        self.area.lines().get(row).map(String::as_str).unwrap_or("")
    }

    fn at_first_line(&self) -> bool {
        self.area.cursor().0 == 0
    }

    fn at_last_line(&self) -> bool {
        self.area.cursor().0 + 1 >= self.area.lines().len()
    }

    /// Mark that the user is editing their own text again, so `Down` no longer
    /// walks forward through history into somebody else's prompt.
    fn editing(&mut self) {
        self.recalled = None;
    }

    fn remember(&mut self, text: &str) {
        // A prompt repeated back to back is one entry. Walking past five copies of
        // the same line is the most annoying thing a history can do.
        if self.history.last().map(String::as_str) != Some(text) {
            self.history.push(text.to_string());
        }
    }

    fn recall_older(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.recalled {
            None => {
                // The collapsed form, not the expanded one. The draft is put
                // back into the composer verbatim when the walk returns, and
                // stashing the expanded text here would hand the operator the
                // whole file back in two rows — the flood the placeholder exists
                // to prevent, arriving by a different door.
                self.draft = self.typed();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.recalled = Some(next);
        let text = self.history[next].clone();
        self.replace(&text);
    }

    fn recall_newer(&mut self) {
        let Some(index) = self.recalled else {
            return;
        };
        if index + 1 < self.history.len() {
            self.recalled = Some(index + 1);
            let text = self.history[index + 1].clone();
            self.replace(&text);
        } else {
            self.recalled = None;
            let draft = std::mem::take(&mut self.draft);
            self.replace(&draft);
        }
    }

    /// Swap what is in the composer for `text`, keeping everything the walk
    /// through history is holding.
    ///
    /// The pasted blocks are carried across the `clear` along with `recalled` and
    /// `draft`, and for the same reason: the draft this walk will come back to
    /// still has its placeholders in it, and a block dropped here would leave one
    /// of them standing for nothing. A recalled *history* entry needs none of
    /// them — history keeps prompts whole — so the stale entries simply never
    /// match, and `clear` drops them when the prompt is done.
    /// The placeholder immediately before the cursor, if the cursor is at the
    /// end of one.
    ///
    /// Backspacing through `[pasted text #4, 366 characters]` one character at a
    /// time is thirty-five presses to remove one thing the operator thinks of as
    /// one thing — and the first press already breaks it, because a placeholder
    /// is matched by its exact text and an edited one stops standing for
    /// anything.
    fn placeholder_before_cursor(&self) -> Option<(String, usize)> {
        let (row, column) = self.area.cursor();
        let line = self.area.lines().get(row)?;
        let before: String = line.chars().take(column).collect();
        // **The trailing space belongs to the thing before it.** An image marker
        // is written as `[Image #1] ` so the next word does not run into the
        // bracket, and the cursor sits after that space — so a deletion took the
        // space first and then, with a word-wise key, ate `1]` off the marker and
        // left `[Image #` on the prompt. The space is counted here and goes with
        // the marker, which is what makes one press remove one thing.
        let trimmed = before.trim_end_matches(' ');
        let spaces = before.chars().count() - trimmed.chars().count();
        self.pastes
            .iter()
            .map(|(placeholder, _)| placeholder)
            .chain(self.markers.iter().map(|(marker, _)| marker))
            .filter(|placeholder| trimmed.ends_with(placeholder.as_str()))
            .max_by_key(|placeholder| placeholder.chars().count())
            .cloned()
            .map(|placeholder| (placeholder, spaces))
    }

    fn replace(&mut self, text: &str) {
        let recalled = self.recalled;
        let draft = std::mem::take(&mut self.draft);
        let pastes = std::mem::take(&mut self.pastes);
        let markers = std::mem::take(&mut self.markers);
        self.clear();
        self.recalled = recalled;
        self.draft = draft;
        self.pastes = pastes;
        self.markers = markers;
        self.area.insert_str(text);
        self.area.move_cursor(CursorMove::End);
    }
}
