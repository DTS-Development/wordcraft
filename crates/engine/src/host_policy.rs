//! The host lock: which commands a PPM-hosted session runs.
//!
//! PPM embeds WordCraft in its own page and owns the files: the parent hands the document over and
//! stores what WordCraft saves. A session locked to its host ([`Session::lock_to_host`]) therefore
//! runs only the commands in [`ALLOWED`] (default deny): everything that opens or replaces the
//! document, creates one, exports, prints or downloads it, loads templates, macros or fonts files,
//! or opens other windows is refused in [`Session::run`], whichever control (ribbon, shortcut,
//! context menu, command search, script) asked. Edits that put things *into* the open document
//! (pictures, files to insert or compare, mail merge, protection, system fonts) are allowed.
//! A new command that is in neither table fails `every_command_is_classified`, so nothing
//! becomes available by being forgotten.
//!
//! Every command has a [`Grade`]: `locked` (no policy turns it on), `off` (off until the parent's
//! policy turns it on: print, Save As, export) or `on` (on until the policy turns it off). The
//! parent sends a [`HostPolicy`] with each open message (`policy.overrides`, command id to bool).
//!
//! The lock is one-way: no command clears it. The app turns it on once at startup from a build
//! switch, so a running page can't be talked out of it.

use serde_json::Value;

use crate::{CmdError, Session};

/// Commands a host-locked session runs: editing and reading the document that is open.
#[rustfmt::skip]
const ALLOWED: &[&str] = &[
    "arrange.align", "arrange.bounds", "arrange.bringForward", "arrange.copyTo", "arrange.group", "arrange.moveWithText", "arrange.nudge",
    "arrange.position", "arrange.rotate", "arrange.rotation", "arrange.selectionPane", "arrange.sendBackward", "arrange.ungroup", "arrange.wrap",
    "arrange.wrapText", "canvas.expand", "canvas.fit", "canvas.member", "canvas.scale", "caret.docEnd", "caret.docStart", "caret.down", "caret.end",
    "caret.home", "caret.left", "caret.pageDown", "caret.pageUp", "caret.paraDown", "caret.paraUp", "caret.pastCitation", "caret.right", "caret.set",
    "caret.up", "caret.wordLeft", "caret.wordRight", "chart.dataLabels", "chart.editData", "chart.legend", "chart.title", "chart.type",
    "design.effects", "design.pageBorders", "design.pageColor", "design.paragraphSpacing", "design.setDefault", "design.styleSet", "design.theme",
    "design.themeColors", "design.themeFonts", "design.themes", "design.watermark", "developer.buildingBlock", "developer.checkBox",
    "developer.chooseItem", "developer.comboBox", "developer.control", "developer.datePicker", "developer.designMode", "developer.dropDown",
    "developer.picture", "developer.plainText", "developer.properties", "developer.removeControl", "developer.repeatingSection", "developer.richText",
    "developer.setDate", "developer.toggleCheckBox", "document.inspect", "document.layout", "document.paragraph", "document.selection",
    "document.setText", "document.text", "draw.addPen", "draw.convertInk", "draw.erase", "draw.eraser", "draw.highlighter", "draw.inkToShape",
    "draw.lasso", "draw.lassoColor", "draw.lassoDelete", "draw.lassoMove", "draw.pen", "draw.pencil", "draw.removePen", "draw.replay", "draw.select",
    "draw.stroke", "edit.advancedFind", "edit.clearClipboard", "edit.clipboardItems", "edit.clipboardPane", "edit.copy", "edit.copyFormat",
    "edit.cut", "edit.deleteClipboardItem", "edit.find", "edit.findNext", "edit.findPrevious", "edit.findReplace", "edit.formatPainter",
    "edit.goBack", "edit.goto", "edit.insertSpike", "edit.paste", "edit.pasteAllClipboard", "edit.pasteClipboardItem", "edit.pasteFormat",
    "edit.pasteMerge", "edit.pasteSpecial", "edit.pasteText", "edit.redo", "edit.repeat", "edit.replace", "edit.replaceAll", "edit.spike",
    "edit.undo", "edit.undoHistory", "edit.undoTo", "equation.backspace", "equation.convert", "equation.delete", "equation.display", "equation.edit",
    "equation.enter", "equation.exit", "equation.gallery", "equation.get", "equation.inputFormat", "equation.insertStructure",
    "equation.insertSymbol", "equation.justify", "equation.move", "equation.normalText", "equation.number", "equation.set", "equation.structure",
    "equation.structureActions", "equation.type", "fields.lock", "fields.next", "fields.previous", "fields.selected", "fields.setCode",
    "fields.toggleCode", "fields.unlink", "fields.unlock", "file.accessibility", "file.compatibility", "file.inspect", "file.options",
    "file.privacy", "file.privacyOptions", "file.save", "file.setAuthor", "font.list", "font.remove",
    "format.allCaps", "format.bold", "format.border", "format.changeCase", "format.charStyle", "format.clear", "format.color",
    "format.doubleStrikethrough", "format.doubleUnderline", "format.emboss", "format.enclose", "format.engrave", "format.font", "format.fontDialog",
    "format.growFont", "format.growFont1", "format.hidden", "format.highlight", "format.italic", "format.outline", "format.phonetic",
    "format.position", "format.resetChar", "format.scale", "format.set", "format.shading", "format.shadow", "format.shrinkFont", "format.shrinkFont1",
    "format.size", "format.smallCaps", "format.spacing", "format.state", "format.strikethrough", "format.subscript", "format.superscript",
    "format.textEffects", "format.underline", "format.wordUnderline", "hf.next", "hf.position", "hf.previous", "insert.autoText", "insert.blankPage",
    "insert.bookmark", "insert.canvas", "insert.chart", "insert.closeHeader", "insert.coverPage", "insert.createFooter", "insert.createHeader",
    "insert.crossReference", "insert.dateField", "insert.dateTime", "insert.deleteBookmark", "insert.docProperty", "insert.dropCap",
    "insert.editFooter", "insert.editHeader", "insert.equation", "insert.field", "insert.footer", "insert.header", "insert.horizontalLine",
    "insert.icon", "insert.link", "insert.object", "insert.pageBreak", "insert.pageField", "insert.pageNumber", "insert.picture", "insert.quickParts",
    "insert.removeEmptyHeaders", "insert.removeFooter", "insert.removeHeader", "insert.removeLink", "insert.shape", "insert.signatureLine",
    "insert.smartArt", "insert.spreadsheet", "insert.symbol", "insert.table", "insert.textBox", "insert.textFromFile", "insert.timeField",
    "insert.wordArt", "layout.break", "layout.columns", "layout.differentFirstPage", "layout.differentOddEven", "layout.documentGrid",
    "layout.hyphenation", "layout.lineNumbers", "layout.linkToPrevious", "layout.manualHyphenation", "layout.margins", "layout.orientation",
    "layout.pageNumberFormat", "layout.pageSetup", "layout.section", "layout.sectionRtl", "layout.size", "layout.textDirection",
    "layout.verticalAlign", "link.targets", "list.define", "list.get", "list.multilevelPreset", "mailings.addressBlock", "mailings.checkErrors",
    "mailings.editRecipients", "mailings.envelopes", "mailings.filterRecipients", "mailings.findRecipient", "mailings.finish",
    "mailings.greetingLine", "mailings.highlightFields", "mailings.insertField", "mailings.labels", "mailings.matchFields", "mailings.next",
    "mailings.preview", "mailings.previous", "mailings.recipients", "mailings.rules", "mailings.sortRecipients", "mailings.start", "object.altText",
    "outline.close", "outline.collapse", "outline.demote", "outline.demoteToBody", "outline.expand", "outline.firstLineOnly", "outline.level",
    "outline.moveDown", "outline.moveUp", "outline.promote", "outline.promoteToHeading1", "outline.selectSubtree", "outline.showAll",
    "outline.showFormatting", "outline.showLevel", "outline.showLevel1", "outline.showLevel2", "outline.showLevel3", "outline.showLevel4",
    "outline.showLevel5", "outline.showLevel6", "outline.showLevel7", "outline.showLevel8", "outline.showLevel9", "outline.state", "outline.toggle",
    "para.addSpaceBefore", "para.align", "para.alignCenter", "para.alignLeft", "para.alignRight", "para.asianTypography", "para.borders",
    "para.bullets", "para.defineBullet", "para.defineNumber", "para.demote", "para.dialog", "para.distribute", "para.double", "para.hangingIndent",
    "para.heading1", "para.heading2", "para.heading3", "para.indent", "para.indents", "para.justify", "para.keepLines", "para.keepNext",
    "para.lineSpacing", "para.listLevel", "para.ltr", "para.moveDown", "para.moveUp", "para.multilevel", "para.normal", "para.numbering",
    "para.oneAndHalf", "para.outdent", "para.outlineLevel", "para.pageBreakBefore", "para.promote", "para.removeHanging", "para.removeSpaceAfter",
    "para.reset", "para.restartNumbering", "para.rtl", "para.set", "para.setNumberingValue", "para.shading", "para.single", "para.sort",
    "para.spacing", "para.style", "para.tabs", "para.widowControl", "picture.altText", "picture.border", "picture.change", "picture.color",
    "picture.compress", "picture.corrections", "picture.crop", "picture.effects", "picture.iconColor", "picture.removeBackground", "picture.reset",
    "picture.size", "picture.style", "picture.transparency", "readAloud.next", "readAloud.playPause", "readAloud.previous", "readAloud.skipCitations",
    "readAloud.speed", "readAloud.status", "readAloud.stop", "references.addText", "references.bibliography", "references.caption",
    "references.citation", "references.citationStyle", "references.convertNotes", "references.endnote", "references.footnote", "references.index",
    "references.markCitation", "references.markEntry", "references.nextFootnote", "references.noteOptions", "references.notes",
    "references.removeToc", "references.researcher", "references.sources", "references.tableOfAuthorities", "references.tableOfFigures",
    "references.toc", "references.updateAuthorities", "references.updateFields", "references.updateFigures", "references.updateIndex",
    "references.updateToc", "review.accept", "review.acceptAll", "review.addToDictionary", "review.applySuggestion", "review.changes",
    "review.combine", "review.comments", "review.compare", "review.deleteComment", "review.editComment", "review.editor", "review.hideInk",
    "review.ignoreAll", "review.issues", "review.language", "review.markup", "review.newComment", "review.nextChange", "review.nextComment",
    "review.previousChange", "review.previousComment", "review.proofing", "review.readAloud", "review.reject", "review.rejectAll", "review.reply",
    "review.resolveComment", "review.showMarkup", "review.spelling", "review.suggestions", "review.thesaurus",
    "review.trackChanges", "review.trackingOptions", "review.wordCount", "select.addObject", "select.all", "select.collapse", "select.column",
    "select.dragUnit", "select.extend", "select.line", "select.objects", "select.paragraph", "select.range", "select.sentence", "select.similar",
    "select.text", "select.word", "shape.alignText", "shape.breakLink", "shape.change", "shape.connect", "shape.effects", "shape.fill", "shape.link",
    "shape.outline", "shape.textDirection", "smartArt.addShape", "smartArt.colors", "smartArt.demote", "smartArt.items", "smartArt.layout",
    "smartArt.promote", "smartArt.reset", "smartArt.textPane", "styles.addToGallery", "styles.apply", "styles.copyFrom", "styles.create",
    "styles.delete", "styles.inspect", "styles.inspector", "styles.inspectorClear", "styles.list", "styles.manage", "styles.modify",
    "styles.organizer", "styles.pane", "styles.rename", "styles.setVisibility", "styles.updateToMatch", "table.altText", "table.autofit",
    "table.borderPainter", "table.borders", "table.cellAlign", "table.cellMargins", "table.columnWidth", "table.deleteCells", "table.deleteColumn",
    "table.deleteRow", "table.deleteStyle", "table.deleteTable", "table.distributeColumns", "table.distributeRows", "table.draw", "table.eraser",
    "table.fitColumn", "table.formula", "table.fromText", "table.insertAt", "table.insertColumnLeft", "table.insertColumnRight",
    "table.insertRowAbove", "table.insertRowBelow", "table.look", "table.merge", "table.modifyStyle", "table.moveDrag", "table.newStyle",
    "table.properties", "table.quick", "table.repeatHeader", "table.resizeDrag", "table.rowHeight", "table.rtl", "table.selectAt", "table.selectCell",
    "table.selectColumn", "table.selectRow", "table.selectTable", "table.shading", "table.sort", "table.split", "table.splitTable", "table.style",
    "table.textDirection", "table.toText", "table.viewGridlines", "text.backTab", "text.backspace", "text.columnBreak", "text.copyright",
    "text.delete", "text.deleteWordBack", "text.deleteWordForward", "text.ellipsis", "text.emDash", "text.insert", "text.lineBreak", "text.nbHyphen",
    "text.nbsp", "text.newParagraph", "text.optionalHyphen", "text.overtype", "text.pageBreak", "text.registered", "text.tab", "text.toggleUnicode",
    "text.trademark", "text.typeCode", "tools.attachTemplate", "tools.autocorrect", "tools.customizeKeyboard", "tools.customizeRibbon", "tools.detachTemplate", "tools.linkStyles", "tools.search",
    "tools.templates", "tools.updateStyles",
    "view.accessibilityPane", "view.altTextPane", "view.commentsPane", "view.darkMode", "view.draft", "view.fieldCodes", "view.focus",
    "view.fullScreen", "view.gridlines", "view.hideWhiteSpace", "view.immersive", "view.interfaceTheme", "view.marks", "view.multiplePages",
    "view.navigationPane", "view.numeral", "view.onePage", "view.outline", "view.pageWidth", "view.printLayout", "view.readColumnWidth",
    "view.readLineSpacing", "view.readMode", "view.readTextSize", "view.ruler", "view.sideToSide", "view.split", "view.state", "view.stylesPane",
    "view.vertical", "view.webLayout", "view.zoom", "view.zoom100", "view.zoomIn", "view.zoomOut", "wordArt.style", "wordArt.textFill",
    "wordArt.textOutline", "wordArt.transform",
];

/// There is no revert command in WordCraft (the registry has none), so none is listed here.
///
/// Commands no policy turns on: they open, create, close or replace the document, keep copies of
/// it, load templates, macros or font files, open other windows, or put a password or an editing
/// restriction on it. `file.encrypt` also removes a password: one set here is applied by the next
/// plain Save, so an editor could lock the parent's copy for everyone else. `review.restrict` and
/// `file.protect` (which runs it) would do the same with read-only / forms protection; `file.info`
/// and `file.properties` belong to the Info page, which is hidden.
#[rustfmt::skip]
const LOCKED: &[&str] = &[
    "file.autosave", "file.close", "file.encrypt", "file.info", "file.new", "file.newFromTemplate", "file.open", "file.properties", "file.protect", "file.recover", "file.rename", "file.share", "file.versions",
    "font.add", "font.loadSystem", "review.restrict", "tools.macros", "tools.recordMacro", "view.newWindow",
];

/// Commands that are off until the parent's policy turns them on: printing, Save As and export.
#[rustfmt::skip]
const OFF: &[&str] = &[
    "file.exportPdf", "file.exportPng", "file.print", "file.printMarkup", "file.printPdf", "file.saveAs", "file.saveTemplate",
];

/// Whether a command can be turned on or off by the parent's policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    /// Never available in PPM.
    Locked,
    /// Off by default; the policy may turn it on.
    Off,
    /// On by default; the policy may turn it off.
    On,
}

impl Grade {
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::Locked => "locked",
            Grade::Off => "off",
            Grade::On => "on",
        }
    }
}

/// Per-command switches the parent sends with an open message (`policy.overrides`): only commands
/// changed from their default are listed. The default (also what a missing or malformed policy
/// means) changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostPolicy {
    overrides: std::collections::BTreeMap<String, bool>,
    /// View-only open (`readOnly`): the document takes no edit and nothing is saved. Not a
    /// command switch, so no override turns it off, and it applies even to an unlocked session.
    read_only: bool,
    /// `chrome: false`: the app hides its title bar, ribbon, status bar, side panes and ruler.
    hide_chrome: bool,
    /// `zoomControls`: with the chrome hidden, a small zoom control stays over the page.
    zoom_controls: bool,
}

/// Most overrides one open message may carry (the contract allows 4000); the first ones that
/// are valid are kept.
pub const MAX_OVERRIDES: usize = 4096;

impl HostPolicy {
    /// The policy of an open message's `policy` field: `{"overrides": {"<id>": bool}}`. Anything
    /// else (no policy, no overrides, not an object) changes nothing.
    pub fn from_message(policy: Option<&Value>) -> Self {
        let Some(map) = policy.and_then(|p| p.get("overrides")).and_then(Value::as_object) else { return Self::default() };
        Self::from_entries(map.iter().map(|(id, v)| (id.clone(), v.as_bool())))
    }

    /// The overrides as the parent wrote them (`None` for a value that is not a boolean). Entries
    /// that are not valid are skipped *before* the limit counts, so junk can't push out a switch.
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Option<bool>)>) -> Self {
        Self::new(entries.into_iter().filter_map(|(id, on)| Some((id, on?))).take(MAX_OVERRIDES))
    }

    pub fn new(overrides: impl IntoIterator<Item = (String, bool)>) -> Self {
        Self { overrides: overrides.into_iter().collect(), ..Self::default() }
    }

    /// This policy for a view-only open: `read_only`, `chrome` (false hides the chrome) and
    /// `zoom_controls` come from the open message's own fields, per document.
    pub fn with_view(mut self, read_only: bool, chrome: bool, zoom_controls: bool) -> Self {
        self.read_only = read_only;
        self.hide_chrome = !chrome;
        self.zoom_controls = zoom_controls;
        self
    }

    /// Whether the document is open view-only.
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the title bar, ribbon, status bar, side panes and ruler are hidden.
    pub fn chrome_hidden(&self) -> bool {
        self.hide_chrome
    }

    /// Whether the zoom control shows over the page when the chrome is hidden.
    pub fn zoom_controls(&self) -> bool {
        self.zoom_controls
    }

    /// Whether a command of this `grade` is available: never when locked, else the override if the
    /// parent sent one for `id`, else the grade's default.
    pub fn allows(&self, id: &str, grade: Grade) -> bool {
        match grade {
            Grade::Locked => false,
            Grade::Off | Grade::On => self.overrides.get(id).copied().unwrap_or(grade == Grade::On),
        }
    }

    /// [`Self::allows`] for an engine command (false for an id in no table).
    pub fn command_on(&self, id: &str) -> bool {
        grade(id).is_some_and(|g| self.allows(id, g))
    }
}

/// Commands a view-only session refuses besides every mutating one: Save (nothing is written,
/// whatever the parent's policy) and the ones that change the document without being marked
/// `mutates`. `read_only_session_refuses_every_edit` finds the rest.
const READ_ONLY_REFUSED: &[&str] =
    &["file.save", "file.saveAs", "file.autosave", "insert.editFooter", "insert.editHeader", "edit.undo", "edit.undoTo", "edit.redo", "edit.repeat"];

/// The grade of engine command `id`; `None` for one in no table (refused until it is added, and
/// `every_command_is_classified` fails).
pub fn grade(id: &str) -> Option<Grade> {
    if ALLOWED.contains(&id) {
        Some(Grade::On)
    } else if OFF.contains(&id) {
        Some(Grade::Off)
    } else if LOCKED.contains(&id) {
        Some(Grade::Locked)
    } else {
        None
    }
}

/// `file.save` params that change what the parent receives: another name or format. Allowed only
/// when Save As (`file.saveAs`) is on. A `password` is refused whatever the policy ([`check`]).
const SAVE_PARAMS_REFUSED: [&str; 2] = ["path", "format"];

/// What a refusal says (the PPM interface is Korean): the command or page is switched off for the parent.
pub const NOT_AVAILABLE: &str = "PPM에서는 사용할 수 없습니다";

/// What a view-only session says when it refuses an edit.
pub const READ_ONLY_TEXT: &str = "보기 전용 문서입니다";

/// `Err` says why a locked session refuses `id` with `params` under `policy`: the command's grade
/// and the parent's override decide, and Save to another name, format or password needs Save As.
pub fn check(id: &str, params: &Value, policy: &HostPolicy) -> Result<(), String> {
    if !policy.command_on(id) {
        return Err(NOT_AVAILABLE.to_string());
    }
    // A password on a save is `file.encrypt` by another route: locked, whatever Save As is set to.
    if matches!(id, "file.save" | "file.saveAs") && params.get("password").is_some_and(|v| !v.is_null()) {
        return Err("PPM에서는 문서를 다른 이름, 형식, 암호로 저장할 수 없습니다".to_string());
    }
    if id == "file.save"
        && !policy.command_on("file.saveAs")
        && let Some(p) = params.as_object()
        && SAVE_PARAMS_REFUSED.iter().any(|k| p.get(*k).is_some_and(|v| !v.is_null()))
    {
        return Err("PPM에서는 문서를 다른 이름, 형식, 암호로 저장할 수 없습니다".to_string());
    }
    Ok(())
}

impl Session {
    /// Lock this session to its host (see the module docs). There is no way back.
    pub fn lock_to_host(&mut self) {
        self.host_lock = true;
    }

    pub fn is_host_locked(&self) -> bool {
        self.host_lock
    }

    /// Set the parent's per-command switches (each open message sends its own).
    pub fn set_host_policy(&mut self, policy: HostPolicy) {
        self.host_policy = policy;
    }

    pub fn host_policy(&self) -> &HostPolicy {
        &self.host_policy
    }

    /// Let the next `file.open` of exactly this document name (with bytes) through the lock: the
    /// parent page's document. The app arms it when the parent sends a file, because the user's
    /// own Open stays blocked. It is used up by that open (or [`Self::disarm_host_open`]).
    pub fn arm_host_open(&mut self, name: &str) {
        self.host_open = Some(name.to_string());
    }

    /// Whether a parent document is armed and waiting (for a Save Changes answer, say).
    pub fn host_open_pending(&self) -> bool {
        self.host_open.is_some()
    }

    pub fn disarm_host_open(&mut self) {
        self.host_open = None;
    }

    /// `Err` when the lock refuses `id` with `params`; always `Ok` for an unlocked session.
    pub fn host_check(&self, id: &str, params: &Value) -> Result<(), String> {
        if !self.host_lock {
            return Ok(());
        }
        let armed = id == "file.open"
            && self.host_open.as_deref().is_some_and(|n| params.get("path").and_then(Value::as_str) == Some(n))
            && params.get("data").is_some_and(Value::is_string);
        if armed { Ok(()) } else { check(id, params, &self.host_policy) }
    }

    /// `Err` when a view-only session refuses `id`: it edits (`mutates`), saves, or is listed in
    /// [`READ_ONLY_REFUSED`]. Checked after the lock and before anything runs, so a policy that
    /// turns an edit command on does not get it past. `file.open` is neither, so the parent's
    /// next document opens (the options change with that open).
    pub fn read_only_check(&self, id: &str, mutates: bool) -> Result<(), String> {
        if !self.host_policy.read_only() {
            return Ok(());
        }
        if mutates || READ_ONLY_REFUSED.contains(&id) {
            return Err(READ_ONLY_TEXT.to_string());
        }
        Ok(())
    }

    /// [`Self::host_check`] as the error [`Session::run`] returns; uses up an armed open.
    pub(crate) fn host_gate(&mut self, id: &str, params: &Value) -> Result<(), CmdError> {
        self.host_check(id, params).map_err(CmdError::Disabled)?;
        if id == "file.open" && self.host_lock {
            self.host_open = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Session;

    fn locked() -> Session {
        let mut s = Session::new(wordcraft_doc::Document::new());
        s.lock_to_host();
        s
    }

    /// Drift: every registered command is in exactly one table, and every table entry is real.
    #[test]
    fn every_command_is_classified() {
        let registry = crate::cmd::registry();
        let ids: Vec<&str> = registry.all().iter().map(|c| c.id).collect();
        let unclassified: Vec<&&str> = ids.iter().filter(|id| grade(id).is_none()).collect();
        assert!(unclassified.is_empty(), "classify these in host_policy.rs (ALLOWED / OFF / LOCKED): {unclassified:?}");
        for (name, table) in [("ALLOWED", ALLOWED), ("OFF", OFF), ("LOCKED", LOCKED)] {
            let gone: Vec<&&str> = table.iter().filter(|id| !ids.contains(id)).collect();
            assert!(gone.is_empty(), "{name} lists commands that no longer exist: {gone:?}");
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in ALLOWED.iter().chain(OFF).chain(LOCKED) {
            assert!(seen.insert(*id), "{id} is in two tables or twice in one");
        }
    }

    fn overrides(pairs: &[(&str, bool)]) -> HostPolicy {
        HostPolicy::new(pairs.iter().map(|(k, v)| (k.to_string(), *v)))
    }

    /// `true` turns an off command on, `false` turns an on command off, and locked commands ignore both.
    #[test]
    fn overrides_switch_commands_by_grade() {
        let none = HostPolicy::default();
        for id in OFF {
            assert!(check(id, &json!({}), &none).is_err(), "{id} is off by default");
            assert!(check(id, &json!({}), &overrides(&[(id, true)])).is_ok(), "{id}");
            assert!(check(id, &json!({}), &overrides(&[(id, false)])).is_err(), "{id}");
        }
        assert!(check("format.bold", &json!({}), &none).is_ok());
        assert!(check("format.bold", &json!({}), &overrides(&[("format.bold", false)])).is_err());
        assert!(check("format.bold", &json!({}), &overrides(&[("format.bold", true)])).is_ok());
        let all_true = HostPolicy::new(LOCKED.iter().map(|id| (id.to_string(), true)));
        for id in LOCKED {
            assert!(check(id, &json!({}), &all_true).is_err(), "{id} is locked");
        }
        assert!(check("not.acommand", &json!({}), &overrides(&[("not.acommand", true)])).is_err(), "unknown ids stay refused");
    }

    /// Everything that opens, replaces, exports or prints the document, or loads templates,
    /// macros or font files, is refused with no policy.
    #[test]
    fn file_and_platform_commands_are_refused() {
        let mut s = locked();
        for id in [
            "file.new",
            "file.open",
            "file.saveAs",
            "file.exportPdf",
            "file.exportPng",
            "file.print",
            "file.printPdf",
            "file.newFromTemplate",
            "file.saveTemplate",
            "file.recover",
            "file.autosave",
            "file.close",
            "file.share",
            "file.rename",
            "file.versions",
            "tools.macros",
            "tools.recordMacro",
            "view.newWindow",
        ] {
            let before = s.doc.plain_text(wordcraft_doc::StoryRef::Body);
            let r = s.run(id, &json!({"path": "/tmp/x.docx", "data": "AAAA", "text": "x"}));
            assert!(matches!(r, Err(CmdError::Disabled(_))), "{id} must be refused, got {r:?}");
            assert_eq!(s.doc.plain_text(wordcraft_doc::StoryRef::Body), before, "{id} changed the document");
        }
    }

    /// Edits that put things into the open document are not refused by the lock.
    #[test]
    fn edits_into_the_document_are_allowed() {
        for id in [
            "insert.picture",
            "insert.textFromFile",
            "insert.object",
            "insert.spreadsheet",
            "picture.change",
            "font.remove",
            "review.compare",
            "review.combine",
            "tools.customizeKeyboard",
            "tools.customizeRibbon",
            "developer.picture",
            "tools.attachTemplate",
            "tools.detachTemplate",
            "tools.linkStyles",
            "tools.updateStyles",
            "tools.templates",
            "mailings.start",
            "mailings.finish",
            "mailings.recipients",
        ] {
            assert!(check(id, &json!({}), &HostPolicy::default()).is_ok(), "{id}");
        }
    }

    /// The policy of a message is read leniently and capped after the invalid entries are dropped.
    #[test]
    fn a_message_policy_is_read_leniently() {
        let none = HostPolicy::default();
        for bad in [
            None,
            Some(json!(null)),
            Some(json!([1])),
            Some(json!({})),
            Some(json!({"overrides": [true]})),
            Some(json!({"overrides": "x"})),
            Some(json!({"allow": ["print"]})),
        ] {
            assert_eq!(HostPolicy::from_message(bad.as_ref()), none, "{bad:?}");
        }
        let p =
            HostPolicy::from_message(Some(&json!({"overrides": {"file.print": true, "format.bold": false, "format.italic": "no", "file.open": 1}})));
        assert!(p.command_on("file.print") && !p.command_on("format.bold") && p.command_on("format.italic"), "a non-boolean is skipped");
        assert!(!p.command_on("file.open"));
        // 5000 junk entries first do not push out the valid switch after them.
        let junk = (0..5000).map(|i| (format!("junk{i}"), None));
        let p = HostPolicy::from_entries(junk.chain([("format.bold".to_string(), Some(false))]));
        assert!(!p.command_on("format.bold"));
        let many = (0..MAX_OVERRIDES + 10).map(|i| (format!("id{i}"), Some(true)));
        assert_eq!(HostPolicy::from_entries(many).overrides.len(), MAX_OVERRIDES);
    }

    /// A user's own fonts (Add Font, Load System Installed Fonts) are locked: the host page brings
    /// the fonts (`wordcraft:add-fonts`), as /builder lets only the administrator's list in.
    #[test]
    fn a_users_own_fonts_are_locked_for_every_policy() {
        let mut s = locked();
        for pol in [HostPolicy::default(), overrides(&[("font.add", true), ("font.loadSystem", true)])] {
            s.set_host_policy(pol);
            for (id, params) in [("font.add", json!({})), ("font.add", json!({"name": "x.ttf", "data": ""})), ("font.loadSystem", json!({}))] {
                let r = s.run(id, &params);
                assert!(matches!(r, Err(CmdError::Disabled(_))), "{id}: {r:?}");
            }
        }
        assert_eq!(grade("font.add"), Some(Grade::Locked));
        assert_eq!(grade("font.loadSystem"), Some(Grade::Locked));
    }

    /// Encrypting is locked: a password set there would be applied by the next plain Save and
    /// lock the parent's copy; no policy turns it on, and removing a password is refused too.
    #[test]
    fn encrypting_is_locked_for_every_policy() {
        let mut s = locked();
        let r = s.run("file.encrypt", &json!({"password": "secret"}));
        assert!(matches!(r, Err(CmdError::Disabled(_))), "{r:?}");
        assert!(s.password.is_none());
        s.set_host_policy(overrides(&[("file.encrypt", true)]));
        let r = s.run("file.encrypt", &json!({"password": "secret"}));
        assert!(matches!(r, Err(CmdError::Disabled(_))), "{r:?}");
        let r = s.run("file.encrypt", &json!({"password": null}));
        assert!(matches!(r, Err(CmdError::Disabled(_))), "{r:?}");
        assert!(s.password.is_none());
        assert_eq!(grade("file.encrypt"), Some(Grade::Locked));
    }

    /// Restrict Editing (and Protect Document, which runs it) is locked: a read-only or forms
    /// protection set here would lock the parent's copy for everyone else. Nothing changes.
    #[test]
    fn restricting_editing_is_locked_for_every_policy() {
        let mut s = locked();
        for pol in [HostPolicy::default(), overrides(&[("review.restrict", true), ("file.protect", true)])] {
            s.set_host_policy(pol);
            for (id, params) in [
                ("review.restrict", json!({"mode": "readOnly"})),
                ("review.restrict", json!({"mode": "forms", "password": "pw"})),
                ("review.restrict", json!({"exception": "add"})),
                ("review.restrict", json!({"stop": true})),
                ("file.protect", json!({"mode": "readOnly"})),
            ] {
                let r = s.run(id, &params);
                assert!(matches!(r, Err(CmdError::Disabled(_))), "{id} {params}: {r:?}");
            }
            assert!(s.doc.settings.protection.is_none(), "{:?}", s.doc.settings.protection);
        }
        for id in ["review.restrict", "file.protect", "file.info", "file.properties"] {
            assert_eq!(grade(id), Some(Grade::Locked), "{id}");
        }
    }

    /// Save to another name or format is Save As's: it follows that command's switch.
    #[test]
    fn save_with_another_name_follows_save_as() {
        let none = HostPolicy::default();
        let save_as = overrides(&[("file.saveAs", true)]);
        assert!(check("file.save", &json!({"path": "other.pdf"}), &none).is_err());
        assert!(check("file.save", &json!({"path": "other.pdf"}), &save_as).is_ok());
        assert!(check("file.save", &json!({}), &none).is_ok());
        // A password is never allowed, even with Save As on.
        for id in ["file.save", "file.saveAs"] {
            assert!(check(id, &json!({"path": "a.docx", "password": "x"}), &save_as).is_err(), "{id}");
        }
        assert!(check("file.saveAs", &json!({"path": "a.docx", "password": null}), &save_as).is_ok());
    }

    /// Editing and saving still work, and an unlocked session is untouched.
    #[test]
    fn editing_and_plain_save_still_work() {
        let mut s = locked();
        s.run("text.insert", &json!({"text": "hello"})).unwrap();
        assert_eq!(s.doc.plain_text(wordcraft_doc::StoryRef::Body), "hello");
        s.run("format.bold", &json!({})).unwrap();
        let off = &HostPolicy::default();
        assert!(check("file.save", &json!({}), off).is_ok());
        assert!(check("file.save", &json!({"path": null}), off).is_ok());
        assert!(check("file.save", &json!({"path": "other.pdf"}), off).is_err(), "Save keeps the parent's name");
        assert!(check("file.save", &json!({"password": "x"}), off).is_err());
        assert!(check("file.save", &json!({"format": "flatopc"}), off).is_err());
        let mut open = Session::new(wordcraft_doc::Document::new());
        assert!(open.run("file.new", &json!({})).is_ok(), "no lock, no refusal");
    }

    /// The parent's own document opens through the lock; any other open does not.
    #[test]
    fn host_open_passes_only_when_armed_for_that_name() {
        let data = json!({"path": "doc.txt", "data": "aGVsbG8="});
        let mut s = locked();
        assert!(s.run("file.open", &data).is_err(), "not armed");
        s.arm_host_open("doc.txt");
        assert!(s.run("file.open", &json!({"path": "other.txt", "data": "aGVsbG8="})).is_err(), "another name");
        assert!(s.run("file.open", &json!({"path": "doc.txt"})).is_err(), "no bytes");
        s.run("file.open", &data).unwrap();
        assert_eq!(s.doc.plain_text(wordcraft_doc::StoryRef::Body), "hello");
        assert!(s.run("file.open", &data).is_err(), "used up by the first open");
    }

    /// A command the lock allows never runs a refused one underneath (it would half-work).
    #[test]
    fn allowed_commands_do_not_trip_the_lock_on_hostile_params() {
        let mut s = locked();
        s.run("text.insert", &json!({"text": "seed text"})).unwrap();
        for id in ALLOWED {
            let r = s.run(id, &json!({"path": "/nonexistent/x", "data": "!!", "text": "t", "value": true}));
            if let Err(CmdError::Disabled(why)) = &r {
                assert!(!why.contains(NOT_AVAILABLE), "{id} ran a command the lock refuses: {why}");
            }
        }
    }
    fn read_only(locked_too: bool) -> Session {
        let mut s = Session::new(crate::sample::sample_document());
        if locked_too {
            s.lock_to_host();
        }
        s.set_host_policy(HostPolicy::default().with_view(true, true, false));
        s
    }

    /// Every command, with junk and with plausible params, leaves a view-only document, its undo
    /// history and the dirty flag alone, locked or not, and a policy that turns edits on changes nothing.
    #[test]
    fn read_only_session_refuses_every_edit() {
        let junk = [
            json!(null),
            json!({}),
            json!({"text": "x", "size": 12, "path": "/tmp/x.docx", "value": true, "rows": 2, "cols": 2}),
            json!({"pos": {"block": 0, "off": 0}, "width": 10.0, "height": 10.0}),
        ];
        let mut leaks = std::collections::BTreeSet::new();
        for locked_too in [false, true] {
            for spec in crate::cmd::registry().all() {
                // These reach outside the session (files, windows); the lock tests cover them.
                if spec.id.starts_with("file.") && !READ_ONLY_REFUSED.contains(&spec.id) {
                    continue;
                }
                if matches!(spec.id, "insert.picture" | "insert.textFromFile") {
                    continue;
                }
                for j in &junk {
                    let mut s = read_only(locked_too);
                    // Everything the parent could switch on is on.
                    let on: Vec<(String, bool)> = ALLOWED.iter().chain(OFF).map(|id| (id.to_string(), true)).collect();
                    s.set_host_policy(HostPolicy::new(on).with_view(true, true, false));
                    let (before, depth) = (s.doc.clone(), s.undo_depth());
                    let r = s.run(spec.id, j);
                    // A locked session may refuse a command first, with its own text.
                    if (spec.mutates || READ_ONLY_REFUSED.contains(&spec.id)) && !matches!(&r, Err(CmdError::Disabled(_))) {
                        leaks.insert(format!("{} ran: {r:?}", spec.id));
                    }
                    if s.doc != before || s.undo_depth() != depth || s.dirty {
                        leaks.insert(format!("{} changed the document, history or dirty flag", spec.id));
                    }
                }
            }
        }
        assert!(leaks.is_empty(), "add these to READ_ONLY_REFUSED: {leaks:#?}");
    }

    /// Undo and redo are not `mutates` but rewrite the document from the history, so a document
    /// that has one (the fuzz test starts empty) must still refuse them.
    #[test]
    fn read_only_refuses_undo_with_history() {
        let mut s = Session::new(crate::sample::sample_document());
        s.run("document.setText", &json!({"text": "Seed"})).unwrap();
        s.set_host_policy(HostPolicy::default().with_view(true, true, false));
        let doc = s.doc.clone();
        for id in ["edit.undo", "edit.redo", "edit.undoTo", "edit.repeat"] {
            assert!(matches!(s.run(id, &json!({"index": 0})), Err(CmdError::Disabled(_))), "{id}");
        }
        assert_eq!(s.doc, doc);
    }

    /// Reading goes on: caret movement, selection, copy, find and zoom run.
    #[test]
    fn read_only_session_still_reads() {
        let mut s = read_only(true);
        for id in ["caret.down", "caret.docEnd", "edit.copy", "edit.find", "view.zoom", "document.text"] {
            let r = s.run(id, &json!({}));
            assert!(!matches!(&r, Err(CmdError::Disabled(m)) if m == READ_ONLY_TEXT), "{id} was refused as an edit: {r:?}");
        }
    }

    /// Save is refused whatever the policy switches, and the parent's next document still opens.
    #[test]
    fn read_only_refuses_save_even_when_save_as_is_on() {
        let mut s = read_only(true);
        s.set_host_policy(HostPolicy::new([("file.saveAs".to_string(), true)]).with_view(true, true, false));
        for id in ["file.save", "file.saveAs"] {
            assert!(matches!(s.run(id, &json!({})), Err(CmdError::Disabled(m)) if m == READ_ONLY_TEXT), "{id}");
        }
        // Auto-save is locked for the host anyway; either refusal is fine.
        assert!(matches!(s.run("file.autosave", &json!({})), Err(CmdError::Disabled(_))));
        s.arm_host_open("next.txt");
        let r = s.run("file.open", &json!({"path": "next.txt", "data": "bmV4dA=="}));
        assert!(r.is_ok(), "the parent's document must open view-only: {r:?}");
    }

    /// A policy without view options is a normal editable session, and the options travel with the policy.
    #[test]
    fn view_options_default_off_and_follow_the_policy() {
        let p = HostPolicy::default();
        assert!(!p.read_only() && !p.chrome_hidden() && !p.zoom_controls());
        let v = p.with_view(true, false, true);
        assert!(v.read_only() && v.chrome_hidden() && v.zoom_controls());
        let mut s = locked();
        s.set_host_policy(v);
        s.set_host_policy(HostPolicy::default());
        assert!(s.run("text.insert", &json!({"text": "x"})).is_ok(), "the next open without options is editable again");
    }
}
