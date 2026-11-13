//! The `postMessage` bridge to a same-origin parent page (`?host=parent`), for pages that embed
//! WordCraft in an `<iframe>`. Messages are plain objects with a `type`:
//!
//! - `wordcraft:ready` (to the parent): `{ version }`, the app is listening; send a file now.
//! - `wordcraft:open` (from the parent): `{ name, bytes, policy? }`, bytes an `ArrayBuffer` or
//!   `Uint8Array`. It opens like a document picked with File › Open or dropped on the window.
//!   `policy: { overrides: { "<command id>": true | false } }` switches commands for this document
//!   (true turns on a command that is off by default, such as `file.print`, `file.saveAs` and
//!   `ui.externalLinks`; false turns one off; locked commands ignore it). Without a valid policy
//!   every command keeps its default.
//! - `wordcraft:save` (to the parent): `{ name, bytes }` (a transferred `ArrayBuffer`) for every
//!   file WordCraft would otherwise download: Save and Save As (a .docx document) and Export
//!   (such as PDF). The parent stores it and reports failures itself.
//! - `wordcraft:dirty` (to the parent): `{ dirty }` whenever unsaved changes appear or go away.
//! - `wordcraft:failed` (to the parent): `{ error }` when the app couldn't start.
//! - `wordcraft:insert-table` (from the parent): `{ id, rows }`, rows an array of arrays of
//!   strings. It inserts a table with those cell texts at the caret, as one undo step.
//! - `wordcraft:table-inserted` (to the parent): `{ id }` when it did.
//! - `wordcraft:table-failed` (to the parent): `{ id, error }` when it couldn't (bad rows, a
//!   document that refuses the table).
//! - `wordcraft:goto` (from the parent): `{ id, location }`, `location` `{ kind: "docx", story:
//!   "body" | "header" | "footer", text, occurrence, total? }`. It selects the `occurrence`-th (from 1)
//!   match of `text` in the story and scrolls it into view (see `host_goto` for how matches
//!   are counted). `total`, if given, is how many matches the parent counted in the story; when
//!   the document has another number nothing is selected and the answer is `not-found`. Nothing
//!   in the document changes. A document still opening answers `not-found`; with 16 requests
//!   waiting the oldest is dropped and answered `error`.
//! - `wordcraft:goto-done` (to the parent): `{ id }` when it did.
//! - `wordcraft:goto-failed` (to the parent): `{ id, reason }`, reason `not-found`,
//!   `unsupported` (a location of another kind or story) or `error` (a malformed request).
//!
//! - `wordcraft:request-bytes` (from the parent): `{ id, purpose }`, purpose `"save"`, `"check"` or
//!   `"preview"`. The document as it is now, in the format Save writes.
//! - `wordcraft:bytes` (to the parent): `{ id, purpose, name, bytes }` (a transferred `ArrayBuffer`).
//!   For `"save"` the document counts as saved, as with File › Save (no `wordcraft:save` goes out);
//!   `"check"` and `"preview"` change nothing in the app.
//! - `wordcraft:bytes-failed` (to the parent): `{ id, purpose, reason }`; `reason` is `not-ready`
//!   (no document from the parent is open yet, or another is still opening), `bad-request`, or the
//!   error text of the failed save.
//! - `wordcraft:add-fonts` (from the parent): `{ id, fonts: [{ family, bytes, format }] }`, each
//!   font file (`format` `"ttf"` or `"otf"`; the file's own data decides) added to the font list and
//!   the layout, before or after a document opens. The font goes by `family` (and by its own
//!   names), and replaces a loaded face of the same family and style, a bundled one too, like a
//!   `FontFace` in the parent page. A family sent before is not added twice.
//! - `wordcraft:fonts-added` (to the parent): `{ id, installed, failed }`, family names.
//!   `installed` means the uploaded font is the one drawn; `failed` fonts are unusable.
//! - Ctrl/Cmd+S sends `wordcraft:save` as File › Save does and never opens the browser's own
//!   "save page" dialog.
//!
//! Only the parent window on the page's own origin is heard or answered. With a parent on another
//! origin there is no bridge: the app runs as if unframed. In host mode the page starts on an
//! empty document (no Start screen, no sample).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wordcraft_engine::host_policy::HostPolicy;
use wordcraft_ui_egui::host_goto::{self, Failure, Goto, Scope};
use wordcraft_ui_egui::host_table::{self, MAX_CELL_CHARS, MAX_COLS, MAX_ROWS};
use wordcraft_ui_egui::{HostPolicySlot, Inbox, Services, WordApp, queue_host_file};

/// The longest user name taken from `?author=`.
const MAX_AUTHOR_CHARS: usize = 200;

/// Most table requests waiting for the next frame; more are dropped.
const MAX_PENDING_TABLES: usize = 16;

/// The longest request id taken from the parent.
const MAX_ID_BYTES: usize = 200;

/// The longest purpose text echoed back to the parent.
const MAX_PURPOSE_CHARS: usize = 200;

/// Most goto requests waiting for the next frame; more are dropped.
const MAX_PENDING_GOTOS: usize = 16;

/// Most byte requests waiting for the next frame; more are dropped.
const MAX_PENDING_BYTES: usize = 16;

/// Most fonts one `add-fonts` message may carry, and the most bytes of all of them together.
const MAX_FONTS_PER_MESSAGE: u32 = 64;
const MAX_FONT_BYTES_PER_MESSAGE: usize = 200 << 20;

/// Why the parent wants the document's bytes.
#[derive(Clone, Copy, PartialEq)]
enum Purpose {
    Save,
    Check,
    Preview,
}

impl Purpose {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "save" => Some(Self::Save),
            "check" => Some(Self::Check),
            "preview" => Some(Self::Preview),
            _ => None,
        }
    }
}

/// A byte request from the parent: its id, and the purpose as sent (kept for the answer) with
/// what it parsed to.
struct BytesRequest {
    id: String,
    purpose_text: String,
    purpose: Option<Purpose>,
}

/// A table the parent asked for: its request id and the rows, or why they couldn't be read.
struct TableRequest {
    id: String,
    rows: Result<Vec<Vec<String>>, String>,
}

/// A place the parent asked to see: its request id and the place, or why it can't be read.
struct GotoRequest {
    id: String,
    goto: Result<Goto, Failure>,
}

#[derive(Clone)]
pub struct Bridge {
    window: web_sys::Window,
    parent: web_sys::Window,
    origin: String,
    /// The unsaved-changes state last reported to the parent.
    reported: Rc<Cell<Option<bool>>>,
    /// Tables from the parent not yet inserted.
    tables: Rc<RefCell<Vec<TableRequest>>>,
    /// Places from the parent not yet shown.
    gotos: Rc<RefCell<Vec<GotoRequest>>>,
    /// Byte requests from the parent not yet answered.
    byte_requests: Rc<RefCell<Vec<BytesRequest>>>,
    /// While a `save` request runs, the file the app hands to `download` is kept here instead of
    /// being sent as `wordcraft:save`.
    capturing: Rc<Cell<bool>>,
    captured: Rc<RefCell<Option<(String, Vec<u8>)>>>,
    /// Font families already added (lower case), so a repeat is not added again.
    fonts: Rc<RefCell<HashSet<String>>>,
}

impl Bridge {
    /// `None` when the page isn't framed by a page on its own origin.
    pub fn new() -> Option<Self> {
        let window = web_sys::window()?;
        let parent = window.parent().ok().flatten()?;
        if js_sys::Object::is(&parent, &window) {
            return None;
        }
        let origin = window.location().origin().ok()?;
        // Reading another origin's location throws, which is `None` too.
        if parent.location().origin().ok()? != origin {
            return None;
        }
        Some(Self {
            window,
            parent,
            origin,
            reported: Rc::default(),
            tables: Rc::default(),
            gotos: Rc::default(),
            byte_requests: Rc::default(),
            capturing: Rc::default(),
            captured: Rc::default(),
            fonts: Rc::default(),
        })
    }

    /// Send saved and exported files to the parent instead of downloading them, report the dirty
    /// flag to it, and hear files from it. A file handed over counts as a finished save: the app
    /// marks the document saved when `download` returns `Ok` and no picker ticket was started.
    pub fn connect(&self, services: &mut Services, ctx: egui::Context) {
        // The parent's documents wait apart from the user's own files (picker, drops).
        let host_inbox = Inbox::default();
        let policy = HostPolicySlot::default();
        services.host_inbox = Some(host_inbox.clone());
        services.host_policy = Some(policy.clone());
        self.listen(host_inbox, policy, ctx);
        self.block_page_save();
        if crate::launch::HOST_BUILD {
            self.block_page_find();
        }
        let bridge = self.clone();
        services.download = Some(Box::new(move |name: &str, bytes: &[u8]| bridge.post_save(name, bytes)));
        // The app tells the dirty flag after each pass; the parent hears it when it changes.
        let bridge = self.clone();
        let previous = services.on_dirty.take();
        services.on_dirty = Some(Box::new(move |dirty| {
            if let Some(previous) = &previous {
                previous(dirty);
            }
            bridge.report_dirty(dirty);
        }));
    }

    /// `?author=`: the host's name for the user signs new comments and tracked changes.
    pub fn set_author(app: &mut WordApp, author: &str) {
        let author: String = author.trim().chars().take(MAX_AUTHOR_CHARS).collect();
        if author.is_empty() {
            return;
        }
        if let Err(e) = app.run("file.setAuthor", serde_json::json!({ "name": author })) {
            log::warn!("?author= ignored: {e}");
        }
    }

    pub fn post_ready(&self) {
        self.send(&[("type", "wordcraft:ready".into()), ("version", env!("CARGO_PKG_VERSION").into())]);
    }

    pub fn post_failed(&self, error: &str) {
        self.send(&[("type", "wordcraft:failed".into()), ("error", error.into())]);
    }

    /// Before each pass: tables from the parent go in at the caret and the parent hears how each went.
    pub fn deliver_tables(&self, app: &mut WordApp) {
        for TableRequest { id, rows } in std::mem::take(&mut *self.tables.borrow_mut()) {
            match rows.and_then(|rows| host_table::insert_table(app, &rows)) {
                Ok(()) => self.send(&[("type", "wordcraft:table-inserted".into()), ("id", id.as_str().into())]),
                Err(error) => self.send(&[("type", "wordcraft:table-failed".into()), ("id", id.as_str().into()), ("error", error.as_str().into())]),
            };
        }
    }

    /// Before each pass: places from the parent are selected and scrolled to, and the parent hears
    /// how each went. A document still opening has no place to go to yet.
    pub fn deliver_gotos(&self, app: &mut WordApp) {
        for GotoRequest { id, goto } in std::mem::take(&mut *self.gotos.borrow_mut()) {
            let done = match goto {
                Ok(_) if !app.host_document_ready() => Err(Failure::NotFound),
                Ok(goto) => host_goto::goto(app, &goto),
                Err(failure) => Err(failure),
            };
            match done {
                Ok(()) => self.send(&[("type", "wordcraft:goto-done".into()), ("id", id.as_str().into())]),
                Err(failure) => self.goto_failed(&id, failure),
            };
        }
    }

    fn goto_failed(&self, id: &str, failure: Failure) -> bool {
        self.send(&[("type", "wordcraft:goto-failed".into()), ("id", id.into()), ("reason", failure.as_str().into())])
    }

    /// Before each pass: answer the parent's byte requests. A `save` runs File › Save, so the
    /// document counts as saved as it does for the user's own Save; the file goes to the parent in
    /// `wordcraft:bytes` instead of `wordcraft:save`. `check` and `preview` only read the document.
    pub fn deliver_bytes(&self, app: &mut WordApp) {
        for BytesRequest { id, purpose_text, purpose } in std::mem::take(&mut *self.byte_requests.borrow_mut()) {
            let Some(purpose) = purpose else {
                self.bytes_failed(&id, &purpose_text, "bad-request");
                continue;
            };
            if !app.host_document_ready() {
                self.bytes_failed(&id, &purpose_text, "not-ready");
                continue;
            }
            let is_save = purpose == Purpose::Save;
            let file = if is_save {
                self.capturing.set(true);
                let saved = app.run("file.save", serde_json::json!({}));
                self.capturing.set(false);
                let captured = self.captured.borrow_mut().take();
                saved.map_err(|e| e.to_string()).and_then(|_| captured.ok_or_else(|| "the save produced no file".to_string()))
            } else {
                app.snapshot_bytes()
            };
            let sent = file.and_then(|(name, bytes)| self.post_bytes(&id, &purpose_text, &name, &bytes));
            if let Err(e) = sent {
                // The save marked the document saved before the file went out: the parent never
                // got it, so the document has unsaved changes again.
                if is_save {
                    app.mark_unsaved();
                }
                self.bytes_failed(&id, &purpose_text, &e);
            }
        }
    }

    fn bytes_failed(&self, id: &str, purpose: &str, reason: &str) {
        self.send(&[("type", "wordcraft:bytes-failed".into()), ("id", id.into()), ("purpose", purpose.into()), ("reason", reason.into())]);
    }

    fn post_bytes(&self, id: &str, purpose: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        let buffer = js_sys::Uint8Array::from(bytes).buffer();
        let message = message(&[
            ("type", "wordcraft:bytes".into()),
            ("id", id.into()),
            ("purpose", purpose.into()),
            ("name", name.into()),
            ("bytes", buffer.clone().into()),
        ])
        .ok_or_else(|| "couldn't build the message".to_string())?;
        self.parent
            .post_message_with_transfer(&message, &self.origin, &js_sys::Array::of1(&buffer))
            .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
    }

    /// Add the fonts of an `add-fonts` message to the font list and answer with how each went.
    /// The font list changes at once; the open document is laid out again on the next pass.
    fn add_fonts(&self, data: &JsValue, id: &str) {
        let (mut installed, mut failed) = (Vec::new(), Vec::new());
        let list = js_sys::Reflect::get(data, &JsValue::from_str("fonts")).ok().filter(js_sys::Array::is_array).map(|f| js_sys::Array::from(&f));
        let mut total = 0usize;
        for (i, font) in list.iter().flat_map(|l| l.iter()).enumerate() {
            let family = string(&font, "family").map(|f| f.trim().to_string()).unwrap_or_default();
            let label = if family.is_empty() { format!("#{i}") } else { family.clone() };
            if i as u32 >= MAX_FONTS_PER_MESSAGE || family.is_empty() {
                failed.push(label);
                continue;
            }
            // A family sent before is already there.
            if self.fonts.borrow().contains(&family.to_lowercase()) {
                installed.push(family);
                continue;
            }
            let Some(bytes) =
                bytes(&font).filter(|b| b.len() <= wordcraft_fonts::MAX_FONT_FILE_BYTES && total + b.len() <= MAX_FONT_BYTES_PER_MESSAGE)
            else {
                failed.push(label);
                continue;
            };
            total += bytes.len();
            // The font goes by the parent's family name and wins over a loaded font of that name.
            match wordcraft_fonts::FontDb::global().install_uploaded_font(&family, &bytes) {
                Ok(_) => {
                    self.fonts.borrow_mut().insert(family.to_lowercase());
                    installed.push(family);
                }
                Err(e) => {
                    log::warn!("font {family} skipped: {e}");
                    failed.push(family);
                }
            }
        }
        log::info!("add-fonts {id}: installed {installed:?}, failed {failed:?}");
        let strings = |v: Vec<String>| v.into_iter().map(JsValue::from).collect::<js_sys::Array>();
        self.send(&[
            ("type", "wordcraft:fonts-added".into()),
            ("id", id.into()),
            ("installed", strings(installed).into()),
            ("failed", strings(failed).into()),
        ]);
    }

    /// Ctrl/Cmd+S must not open the browser's "save page" dialog. eframe stops it for keys the
    /// canvas hears; this also covers focus elsewhere in the frame. The app still gets the key.
    fn block_page_save(&self) {
        let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|event: web_sys::KeyboardEvent| {
            if (event.ctrl_key() || event.meta_key()) && (event.code() == "KeyS" || event.key().eq_ignore_ascii_case("s")) {
                event.prevent_default();
            }
        });
        if self.window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref()).is_ok() {
            on_key.forget();
        }
    }

    /// Cmd/Ctrl+F, G, H and F3 are the editor's Find keys; eframe leaves them to the browser, whose
    /// find bar would open over the editor's. Capture phase, no `stopPropagation`: egui still gets the key.
    fn block_page_find(&self) {
        let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|event: web_sys::KeyboardEvent| {
            if crate::find_keys::blocks_browser_default(
                event.ctrl_key(),
                event.meta_key(),
                event.shift_key(),
                event.alt_key(),
                &event.code(),
                &event.key(),
            ) {
                event.prevent_default();
            }
        });
        let options = web_sys::AddEventListenerOptions::new();
        options.set_capture(true);
        if self
            .window
            .add_event_listener_with_callback_and_add_event_listener_options("keydown", on_key.as_ref().unchecked_ref(), &options)
            .is_ok()
        {
            on_key.forget();
        }
    }

    fn report_dirty(&self, dirty: bool) {
        if self.reported.get() != Some(dirty) && self.send(&[("type", "wordcraft:dirty".into()), ("dirty", dirty.into())]) {
            self.reported.set(Some(dirty));
        }
    }

    fn post_save(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        if self.capturing.get() {
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
            *self.captured.borrow_mut() = Some((name, bytes.to_vec()));
            return Ok(());
        }
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
        let buffer = js_sys::Uint8Array::from(bytes).buffer();
        let message = message(&[("type", "wordcraft:save".into()), ("name", name.as_str().into()), ("bytes", buffer.clone().into())])
            .ok_or_else(|| "couldn't build the message".to_string())?;
        self.parent
            .post_message_with_transfer(&message, &self.origin, &js_sys::Array::of1(&buffer))
            .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
    }

    fn listen(&self, inbox: Inbox, policy_slot: HostPolicySlot, ctx: egui::Context) {
        let parent = self.parent.clone();
        let origin = self.origin.clone();
        let tables = self.tables.clone();
        let gotos = self.gotos.clone();
        let byte_requests = self.byte_requests.clone();
        let bridge = self.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if event.origin() != origin || !event.source().is_some_and(|s| js_sys::Object::is(&s, &parent)) {
                return;
            }
            let data = event.data();
            match string(&data, "type").as_deref() {
                Some("wordcraft:open") => {
                    let name = string(&data, "name").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Document.docx".to_string());
                    let Some(bytes) = bytes(&data) else { return };
                    // The policy goes in with the file (one per file, the queue is capped).
                    queue_host_file(&inbox, &policy_slot, (name, bytes), policy(&data));
                }
                Some("wordcraft:insert-table") => {
                    // Without a usable id there is nobody to answer.
                    let Some(id) = string(&data, "id").filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES) else { return };
                    if tables.borrow().len() < MAX_PENDING_TABLES {
                        tables.borrow_mut().push(TableRequest { id, rows: rows(&data) });
                    }
                }
                Some("wordcraft:goto") => {
                    let Some(id) = string(&data, "id").filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES) else { return };
                    // A full queue drops its oldest request, which hears `error`.
                    let dropped = {
                        let mut queue = gotos.borrow_mut();
                        let dropped = (queue.len() >= MAX_PENDING_GOTOS).then(|| queue.remove(0));
                        queue.push(GotoRequest { id, goto: goto_location(&data) });
                        dropped
                    };
                    if let Some(old) = dropped {
                        bridge.goto_failed(&old.id, Failure::Error);
                    }
                }
                Some("wordcraft:request-bytes") => {
                    let Some(id) = string(&data, "id").filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES) else { return };
                    let purpose_text: String = string(&data, "purpose").unwrap_or_default().chars().take(MAX_PURPOSE_CHARS).collect();
                    if byte_requests.borrow().len() < MAX_PENDING_BYTES {
                        let purpose = Purpose::parse(&purpose_text);
                        byte_requests.borrow_mut().push(BytesRequest { id, purpose_text, purpose });
                    }
                }
                Some("wordcraft:add-fonts") => {
                    let Some(id) = string(&data, "id").filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES) else { return };
                    bridge.add_fonts(&data, &id);
                }
                _ => return,
            }
            ctx.request_repaint();
        });
        if self.window.add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref()).is_ok() {
            // The listener lives as long as the page.
            on_message.forget();
        }
    }

    /// Post a message to the parent; false if it couldn't be sent.
    fn send(&self, fields: &[(&str, JsValue)]) -> bool {
        message(fields).is_some_and(|m| self.parent.post_message(&m, &self.origin).is_ok())
    }
}

fn message(fields: &[(&str, JsValue)]) -> Option<js_sys::Object> {
    let m = js_sys::Object::new();
    for (key, value) in fields {
        js_sys::Reflect::set(&m, &JsValue::from_str(key), value).ok()?;
    }
    Some(m)
}

/// The per-command switches of the open message's `policy.overrides` (`{ "<command id>": bool }`),
/// read as [`HostPolicy::from_entries`] does: a missing or malformed policy changes nothing, a
/// value that is not a boolean is skipped, an id that is no command does nothing.
fn policy(data: &JsValue) -> HostPolicy {
    let get = |from: &JsValue, key: &str| js_sys::Reflect::get(from, &JsValue::from_str(key)).ok();
    let Some(overrides) = get(data, "policy").and_then(|p| get(&p, "overrides")).filter(|o| o.is_object() && !js_sys::Array::is_array(o)) else {
        return HostPolicy::default();
    };
    let entries = js_sys::Object::entries(&js_sys::Object::from(overrides));
    HostPolicy::from_entries(entries.iter().filter_map(|e| {
        let pair = js_sys::Array::from(&e);
        Some((pair.get(0).as_string()?, pair.get(1).as_bool()))
    }))
}

fn string(data: &JsValue, key: &str) -> Option<String> {
    js_sys::Reflect::get(data, &JsValue::from_str(key)).ok()?.as_string()
}

/// The place a `goto` message names: a `docx` location with a known story, a text and an
/// occurrence. Another kind of location is `Unsupported`; anything unreadable is an `Error`.
fn goto_location(data: &JsValue) -> Result<Goto, Failure> {
    let get = |from: &JsValue, key: &str| js_sys::Reflect::get(from, &JsValue::from_str(key)).ok();
    let location = get(data, "location").filter(|l| l.is_object()).ok_or(Failure::Error)?;
    if string(&location, "kind").as_deref() != Some("docx") {
        return Err(Failure::Unsupported);
    }
    let scope = string(&location, "story").and_then(|s| Scope::parse(&s)).ok_or(Failure::Unsupported)?;
    // A text far past the limit is refused before it is copied (`Goto::new` then checks the bytes).
    let text = get(&location, "text")
        .filter(|t| t.dyn_ref::<js_sys::JsString>().is_some_and(|s| s.length() as usize <= host_goto::MAX_TEXT_BYTES))
        .and_then(|t| t.as_string());
    let occurrence = get(&location, "occurrence").and_then(|o| o.as_f64()).filter(|o| o.fract() == 0.0 && *o >= 1.0).map(|o| o as u64);
    // `total` (optional) is how many matches the host counted in the story.
    let total = match get(&location, "total").filter(|t| !t.is_undefined() && !t.is_null()) {
        None => None,
        Some(t) => Some(t.as_f64().filter(|t| t.fract() == 0.0 && *t >= 1.0).map(|t| t as u64).ok_or(Failure::Error)?),
    };
    match (text, occurrence) {
        (Some(text), Some(occurrence)) => Goto::new(scope, &text, occurrence)?.with_total(total),
        _ => Err(Failure::Error),
    }
}

/// `rows` as an array of arrays of strings, no larger than a table may be (checked before the
/// values are copied out).
fn rows(data: &JsValue) -> Result<Vec<Vec<String>>, String> {
    let bad = || "rows must be an array of arrays of strings".to_string();
    let rows = js_sys::Reflect::get(data, &JsValue::from_str("rows")).map_err(|_| bad())?;
    if !js_sys::Array::is_array(&rows) {
        return Err(bad());
    }
    let rows = js_sys::Array::from(&rows);
    if rows.length() as usize > MAX_ROWS {
        return Err(format!("a table needs 1 to {MAX_ROWS} rows"));
    }
    rows.iter()
        .map(|row| {
            if !js_sys::Array::is_array(&row) {
                return Err(bad());
            }
            let row = js_sys::Array::from(&row);
            if row.length() as usize > MAX_COLS {
                return Err(format!("a row needs 1 to {MAX_COLS} cells"));
            }
            row.iter()
                .map(|cell| {
                    // UTF-16 units are never fewer than characters: a longer string is refused before it is copied.
                    let units = cell.dyn_ref::<js_sys::JsString>().map(|s| s.length() as usize).ok_or_else(bad)?;
                    if units > MAX_CELL_CHARS * 2 {
                        return Err(format!("a cell holds at most {MAX_CELL_CHARS} characters"));
                    }
                    cell.as_string().ok_or_else(bad)
                })
                .collect()
        })
        .collect()
}

/// `bytes` as an `ArrayBuffer` or `Uint8Array`.
fn bytes(data: &JsValue) -> Option<Vec<u8>> {
    let bytes = js_sys::Reflect::get(data, &JsValue::from_str("bytes")).ok()?;
    if let Some(array) = bytes.dyn_ref::<js_sys::Uint8Array>() {
        Some(array.to_vec())
    } else if bytes.is_instance_of::<js_sys::ArrayBuffer>() {
        Some(js_sys::Uint8Array::new(&bytes).to_vec())
    } else {
        None
    }
}
