# Pictures

**Drag a picture onto the prompt, or copy it and paste.** That is the whole of
it — there is no command. What lands is `[Image #1]`, and the picture rides the
**next turn and only the next turn**. Paste the same file again to toggle between
the marker and the path it stands for; backspace takes the marker off in one
press, whichever backspace you use. `/image 1` draws the picture itself, at the
bottom, when you want to look at it — a committed row belongs to your terminal's
scrollback, so it cannot be opened in place.

**`/attach` was removed in 0.13.1**, alias and all. It was a command you had to
be told about before you could use the feature, and dropping a picture into the
window is what everyone already does. Typing it is answered the way any other
word that is not a command is.

A path **inside the workspace** is read through io-harness's own workspace, under
the same policy as everything else — its documentation is explicit that this is
the same gate a source read passes and not a second one — so an image the session
may not read is refused exactly the way a file it may not read already is.

A path **outside the workspace** is read directly, and that is deliberate: the
file you point at is almost never inside the repository, and every absolute path
was refused before — which made this unusable for the one thing most people
attach. This is the only read in the product that is not the agent's, and it is
the boundary `!` already crosses when it runs your own shell line. What may be
sent is io-harness's decision too: bmp, tiff, ico, tga and pnm are converted to
PNG on the way in, jpeg, png, gif and webp go as they are, and svg, heic and avif
are refused **by name**, because a refusal that says which format it was is one
you can act on. A provider that does not accept images at all is refused at the
door rather than after you have typed the prompt.

**The agent can look at images in the workspace**, using io-harness's own
`view_image` tool, which enabling its `media` feature switches on. It is bounded
by the same policy as any other read. When it looks, the same picture goes into
your scrollback at that point in the conversation, so you are reading what it
read rather than a path you would have to open yourself.

That is the shape every capability of this kind arrives in, and 0.20.0 adds
twelve more of them: **io-cli cannot take a tool out of io-harness's workspace
tool set**, so a feature this crate turns on is a tool the agent has, and the
only honest thing to do with that is say so. See [Documents](#documents).

A picture is drawn from half blocks — `▀` splits a cell into two halves that are
each about square — fitted to your terminal's width and bounded in height. On
kitty, ghostty, WezTerm and Konsole a PNG is drawn as the **real image** instead,
and on iTerm2 so is a png, jpeg or gif — it decodes the file itself, so it is not
limited to the one format Kitty's transfer takes. Inside tmux or screen it is
always half blocks: passing a graphics protocol through a multiplexer needs
configuration that is off by default, and an escape the terminal cannot read is
unreadable bytes written permanently into your scrollback.

Under `--plain`, under `NO_COLOR`, and with the ASCII glyph set there is no
picture at all — one line naming the file, its format and its size. A half-block
picture is colour carrying the entire meaning, which is the one thing this
interface will not do.

## Documents

**The agent can read and write spreadsheets, Word files, slide decks, PDFs and
barcodes from 0.20.0**, because io-cli turns on io-harness's `documents` feature
— `xlsx`, `docx`, `pptx`, `pdf` and `barcode`. That is twelve tools in its
workspace tool set, and **six of them write**:

| Format | Reads with | Writes with |
| --- | --- | --- |
| Spreadsheets | `xlsx_sheets`, `xlsx_read` | `xlsx_write`, `xlsx_set_cell` |
| Word | `docx_read` | `docx_write` |
| PowerPoint | `pptx_read` | — |
| PDF | `pdf_read` | `pdf_write`, `pdf_watermark`, `pdf_fill_form` |
| Barcodes | `barcode_decode` | — |

Every one of them is a read or a write like any other: the same policy gate, the
same approval prompt answered where it was asked, and the same refusal naming the
act, the target, the rule and the layer. **`xlsx_write` replaces a file that
already exists** — under the write gate, so it is proposed to you before it
happens rather than reported afterwards.

**Which reader runs is decided by the tool the model called, not by the file's
extension.** A `.docx` handed to `pdf_read` is a failed read rather than a guess,
and renaming a file changes nothing about what can be done to it.

**What they do not do**, because a document tool that half-works is worse than
one that is absent:

- **Word is generate-and-read, with no edit in place.** A read followed by a
  write produces a new document out of the text that came back, so comments,
  content controls, fields and vendor extensions that were in the original are
  not in the result. It is the right tool for producing a document and the wrong
  one for touching up somebody else's contract.
- **PowerPoint is read-only.** There is no `pptx_write`, and the table above is
  the whole of it.
- **PDF text extraction is best-effort about reading order**, which is what
  extracting text from a page-description format means. **A scanned page comes
  back with empty text rather than an error**, because there is nothing in it to
  extract — there is no OCR anywhere in this.
- **`xlsx_set_cell` preserves the rest of a workbook in practice rather than by
  guarantee.** It is the tool for changing a value in a sheet of data, and not
  the tool for a workbook heavy with charts, pivot tables or macros.
- **There is no barcode generation**, only decoding.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
