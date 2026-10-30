//! A place the host page asks to see (`wordcraft:goto`): the n-th match of a text in the body,
//! the headers, the footers, the footnotes or the endnotes is selected and scrolled into view.
//!
//! The count is the host's too (the tag check counts the same way): a paragraph's text, tables
//! row by row and cell by cell, matches inside one paragraph, left to right, never overlapping,
//! case-sensitive. The headers (footers) are one story: their parts one after another in the
//! order the file first refers to them (a part's id grows in that order when a file is read). The
//! footnotes (endnotes) are one story too: the notes the body refers to, in the order its paragraphs
//! first refer to them, each once; a note nothing refers to is not counted.
//!
//! Marks the host does not count are left out of a paragraph's text before matching: bookmarks,
//! comment ranges, editable ranges, content control ends and equations (each is one U+FFFC in the
//! model). A match that spans one still selects the whole tag: the selection goes back to the
//! paragraph's own offsets.

use std::borrow::Cow;

use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Document, InlineObject, Paragraph, PartKind, Path, Pos, StoryRef};

use crate::WordApp;

/// The longest text a request may look for, in bytes.
pub const MAX_TEXT_BYTES: usize = 4000;
/// The largest occurrence a request may ask for.
pub const MAX_OCCURRENCE: u64 = 1_000_000;

/// Which stories the count runs over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Body,
    Header,
    Footer,
    Footnote,
    Endnote,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "body" => Some(Self::Body),
            "header" => Some(Self::Header),
            "footer" => Some(Self::Footer),
            "footnote" => Some(Self::Footnote),
            "endnote" => Some(Self::Endnote),
            _ => None,
        }
    }
}

/// A request that passed [`Goto::new`]'s checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Goto {
    pub scope: Scope,
    pub text: String,
    /// Counted from 1.
    pub occurrence: usize,
    /// How many matches the host counted in the story; when the document has another number it
    /// has changed, and nothing is selected.
    pub total: Option<usize>,
}

/// Why a request couldn't be done; `as_str` is what the host page hears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NotFound,
    Unsupported,
    Error,
}

impl Failure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::Unsupported => "unsupported",
            Self::Error => "error",
        }
    }
}

impl Goto {
    /// A request for the `occurrence`-th match of `text`; an empty or oversized text or an
    /// occurrence of 0 or past [`MAX_OCCURRENCE`] is an error, not a not-found.
    pub fn new(scope: Scope, text: &str, occurrence: u64) -> Result<Self, Failure> {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES || occurrence == 0 || occurrence > MAX_OCCURRENCE {
            return Err(Failure::Error);
        }
        Ok(Self { scope, text: text.to_string(), occurrence: occurrence as usize, total: None })
    }

    /// The host's own count of matches (1 or more) to check against.
    pub fn with_total(mut self, total: Option<u64>) -> Result<Self, Failure> {
        if total.is_some_and(|t| t == 0 || t > MAX_OCCURRENCE) {
            return Err(Failure::Error);
        }
        self.total = total.map(|t| t as usize);
        Ok(self)
    }
}

/// The stories of `scope`, in the order the count runs over them.
fn stories(doc: &Document, scope: Scope) -> Vec<StoryRef> {
    let kind = match scope {
        Scope::Body => return vec![StoryRef::Body],
        Scope::Header => PartKind::Header,
        Scope::Footer => PartKind::Footer,
        Scope::Footnote => return noted(doc, PartKind::Footnote),
        Scope::Endnote => return noted(doc, PartKind::Endnote),
    };
    // `parts` is ordered by id.
    doc.parts.iter().filter(|(_, p)| p.kind == kind).map(|(id, _)| StoryRef::Part(*id)).collect()
}

/// The notes of `kind` the body refers to, in the order its paragraphs first refer to them (a
/// note referred to twice counts once; one nothing refers to is left out).
fn noted(doc: &Document, kind: PartKind) -> Vec<StoryRef> {
    let mut out = Vec::<StoryRef>::new();
    for path in doc.para_paths(StoryRef::Body) {
        let Some(p) = doc.para(StoryRef::Body, &path) else { continue };
        for o in &p.objects {
            if let InlineObject::NoteRef { id, .. } = o
                && doc.parts.get(id).is_some_and(|part| part.kind == kind)
                && !out.contains(&StoryRef::Part(*id))
            {
                out.push(StoryRef::Part(*id));
            }
        }
    }
    out
}

/// Whether the host leaves this object out of a paragraph's text.
fn uncounted(o: &InlineObject) -> bool {
    matches!(
        o,
        InlineObject::BookmarkStart { .. }
            | InlineObject::BookmarkEnd { .. }
            | InlineObject::CommentStart { .. }
            | InlineObject::CommentEnd { .. }
            | InlineObject::PermStart { .. }
            | InlineObject::PermEnd { .. }
            | InlineObject::ControlStart { .. }
            | InlineObject::ControlEnd
            | InlineObject::Equation { .. }
    )
}

/// A paragraph's text as the host counts it, with where each piece of it lies in the paragraph.
struct Counted<'a> {
    text: Cow<'a, str>,
    /// (offset in `text`, offset in the paragraph, length) of each stretch kept; empty when
    /// `text` is the paragraph's own.
    pieces: Vec<(usize, usize, usize)>,
}

fn counted(p: &Paragraph) -> Counted<'_> {
    if !p.objects.iter().any(uncounted) {
        return Counted { text: Cow::Borrowed(&p.text), pieces: Vec::new() };
    }
    let (mut text, mut pieces, mut k) = (String::new(), Vec::<(usize, usize, usize)>::new(), 0);
    for (at, c) in p.text.char_indices() {
        if c == OBJ {
            k += 1;
            if p.objects.get(k - 1).is_some_and(uncounted) {
                continue;
            }
        }
        match pieces.last_mut() {
            Some(last) if last.1 + last.2 == at => last.2 += c.len_utf8(),
            _ => pieces.push((text.len(), at, c.len_utf8())),
        }
        text.push(c);
    }
    Counted { text: Cow::Owned(text), pieces }
}

impl Counted<'_> {
    /// The paragraph's offsets of the counted range `start..end`.
    fn original(&self, start: usize, end: usize) -> (usize, usize) {
        if self.pieces.is_empty() {
            return (start, end);
        }
        let from = self.pieces.iter().find(|(t, _, len)| start < t + len).map_or(start, |(t, o, _)| o + (start - t));
        let to = self.pieces.iter().find(|(t, _, len)| end > *t && end <= t + len).map_or(end, |(t, o, _)| o + (end - t));
        (from, to)
    }
}

/// Matches of `text` in each paragraph of `story`, in document order.
fn matches_in(doc: &Document, story: StoryRef, text: &str) -> usize {
    doc.para_paths(story).iter().filter_map(|path| doc.para(story, path)).map(|p| counted(p).text.matches(text).count()).sum()
}

/// How many matches of `text` there are over `scope`.
pub fn count(doc: &Document, scope: Scope, text: &str) -> usize {
    stories(doc, scope).into_iter().map(|s| matches_in(doc, s, text)).sum()
}

/// The story holding the `n`-th match (from 1) of `text` over `scope`, and which match that is
/// within the story.
pub fn locate(doc: &Document, scope: Scope, text: &str, n: usize) -> Option<(StoryRef, usize)> {
    let mut before = 0;
    for story in stories(doc, scope) {
        let here = matches_in(doc, story, text);
        if n <= before + here {
            return Some((story, n - before));
        }
        before += here;
    }
    None
}

/// The paragraph and offsets of the `n`-th match (from 1) of `text` in `story`.
fn place(doc: &Document, story: StoryRef, text: &str, n: usize) -> Option<(Path, usize, usize)> {
    let mut seen = 0;
    for path in doc.para_paths(story) {
        let Some(p) = doc.para(story, &path) else { continue };
        let c = counted(p);
        for (at, _) in c.text.match_indices(text) {
            seen += 1;
            if seen == n {
                let (from, to) = c.original(at, at + text.len());
                return Some((path, from, to));
            }
        }
    }
    None
}

/// Select the match and bring it into view. Nothing changes in the document. A `total` that is not
/// the document's count means the document is not the one the host looked at: not found.
pub fn goto(app: &mut WordApp, req: &Goto) -> Result<(), Failure> {
    let doc = &app.session.doc;
    if req.total.is_some_and(|t| t != count(doc, req.scope, &req.text)) {
        return Err(Failure::NotFound);
    }
    let (story, n) = locate(doc, req.scope, &req.text, req.occurrence).ok_or(Failure::NotFound)?;
    let (path, from, to) = place(doc, story, &req.text, n).ok_or(Failure::NotFound)?;
    let pos = |off| serde_json::to_value(Pos { story, path: path.clone(), off }).map_err(|_| Failure::Error);
    // `select.range` is on the host's allowed list; a finished command follows the caret.
    app.run("select.range", serde_json::json!({ "anchor": pos(from)?, "focus": pos(to)? })).map(|_| ()).map_err(|_| Failure::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_engine::Session;

    fn app(text: &str) -> WordApp {
        WordApp::new(Session::new(Document::from_text(text)), Default::default())
    }

    /// The text the selection covers, in its story.
    fn selected(a: &WordApp) -> (StoryRef, String) {
        let (s, f) = (&a.session.sel.anchor, &a.session.sel.focus);
        let p = a.session.doc.para_at(s).expect("a paragraph");
        (s.story, p.text[s.off..f.off].to_string())
    }

    #[test]
    fn requests_are_checked() {
        assert_eq!(Goto::new(Scope::Body, "", 1), Err(Failure::Error));
        assert_eq!(Goto::new(Scope::Body, "x", 0), Err(Failure::Error));
        assert_eq!(Goto::new(Scope::Body, "x", MAX_OCCURRENCE + 1), Err(Failure::Error));
        assert_eq!(Goto::new(Scope::Body, &"x".repeat(MAX_TEXT_BYTES + 1), 1), Err(Failure::Error));
        assert!(Goto::new(Scope::Body, "{{a}}", 2).is_ok());
        assert_eq!((Scope::parse("header"), Scope::parse("footer"), Scope::parse("notes")), (Some(Scope::Header), Some(Scope::Footer), None));
    }

    #[test]
    fn matches_do_not_overlap_and_count_left_to_right() {
        let mut a = app("aaaa {{x}} {{x}}{{x}}\n{{ x }} {{x}}");
        // "aa" fits twice in "aaaa", not three times.
        assert_eq!(matches_in(&a.session.doc, StoryRef::Body, "aa"), 2);
        let g = Goto::new(Scope::Body, "{{x}}", 4).unwrap();
        goto(&mut a, &g).unwrap();
        let (_, sel) = selected(&a);
        assert_eq!(sel, "{{x}}");
        // The 4th is in the second paragraph (the spaced `{{ x }}` is another text).
        assert_eq!(a.session.sel.anchor.path.0, vec![1]);
        assert_eq!(a.session.sel.anchor.off, "{{ x }} ".len());
        // Case counts.
        assert_eq!(goto(&mut a, &Goto::new(Scope::Body, "{{X}}", 1).unwrap()), Err(Failure::NotFound));
        assert_eq!(goto(&mut a, &Goto::new(Scope::Body, "{{x}}", 5).unwrap()), Err(Failure::NotFound));
    }

    #[test]
    fn the_view_follows_the_selection() {
        let mut a = app("one\ntwo {{x}}");
        a.canvas.scroll_to_caret = false;
        goto(&mut a, &Goto::new(Scope::Body, "{{x}}", 1).unwrap()).unwrap();
        assert!(a.canvas.scroll_to_caret, "the canvas scrolls to the selection on the next pass");
    }

    #[test]
    fn a_host_locked_document_still_goes_to_a_place() {
        let mut a = app("one {{x}}");
        a.session.lock_to_host();
        goto(&mut a, &Goto::new(Scope::Body, "{{x}}", 1).unwrap()).unwrap();
        assert_eq!(selected(&a).1, "{{x}}");
    }

    #[test]
    fn a_table_counts_row_by_row_and_cell_by_cell() {
        let mut a = app("before {{t}}");
        a.session.sel = wordcraft_engine::Selection::caret(a.session.doc.end_of(StoryRef::Body));
        a.run("insert.table", serde_json::json!({"data": [["{{t}} a", "b {{t}}"], ["{{t}}", "{{t}}{{t}}"]]})).unwrap();
        let doc = &a.session.doc;
        // Counted: 1 before the table, then a1, b1, a2, b2 (two).
        let (story, n) = locate(doc, Scope::Body, "{{t}}", 5).unwrap();
        assert_eq!((story, n), (StoryRef::Body, 5));
        goto(&mut a, &Goto::new(Scope::Body, "{{t}}", 3).unwrap()).unwrap();
        // 3rd = row 1, cell 2: the path is [table block, row, cell, block].
        assert_eq!(a.session.sel.anchor.path.0[1..3], [0, 1]);
        goto(&mut a, &Goto::new(Scope::Body, "{{t}}", 5).unwrap()).unwrap();
        assert_eq!(a.session.sel.anchor.path.0[1..3], [1, 1]);
        assert_eq!(a.session.sel.anchor.off, 0);
        goto(&mut a, &Goto::new(Scope::Body, "{{t}}", 6).unwrap()).unwrap();
        assert_eq!((a.session.sel.anchor.path.0[1..3].to_vec(), a.session.sel.anchor.off), (vec![1, 1], "{{t}}".len()));
    }

    #[test]
    fn headers_and_footers_are_one_story_each_in_part_order() {
        let mut a = app("body {{h}}");
        a.run("insert.header", serde_json::json!({"text": "H1 {{h}}"})).unwrap();
        a.run("insert.closeHeader", serde_json::json!({})).unwrap();
        a.run("layout.differentFirstPage", serde_json::json!({"value": true})).unwrap();
        a.run("insert.header", serde_json::json!({"text": "H2 {{h}} {{h}}"})).unwrap();
        a.run("insert.closeHeader", serde_json::json!({})).unwrap();
        a.run("insert.footer", serde_json::json!({"text": "F1 {{h}}"})).unwrap();
        a.run("insert.closeHeader", serde_json::json!({})).unwrap();
        let sect = a.session.doc.last_section.clone();
        let (h_default, h_first) = (sect.headers.default.unwrap(), sect.headers.first.unwrap());
        assert!(h_default < h_first);
        // The count in the body does not see them, nor the footer the headers.
        assert_eq!(locate(&a.session.doc, Scope::Body, "{{h}}", 2), None);
        assert_eq!(locate(&a.session.doc, Scope::Header, "{{h}}", 1), Some((StoryRef::Part(h_default), 1)));
        assert_eq!(locate(&a.session.doc, Scope::Header, "{{h}}", 3), Some((StoryRef::Part(h_first), 2)));
        assert_eq!(locate(&a.session.doc, Scope::Header, "{{h}}", 4), None);
        goto(&mut a, &Goto::new(Scope::Header, "{{h}}", 3).unwrap()).unwrap();
        assert_eq!(selected(&a), (StoryRef::Part(h_first), "{{h}}".to_string()));
        assert_eq!(a.session.sel.anchor.off, "H2 {{h}} ".len());
        goto(&mut a, &Goto::new(Scope::Footer, "{{h}}", 1).unwrap()).unwrap();
        assert_eq!(selected(&a).1, "{{h}}");
        assert_eq!(goto(&mut a, &Goto::new(Scope::Footer, "{{h}}", 2).unwrap()), Err(Failure::NotFound));
        // Going back to the body leaves the header.
        goto(&mut a, &Goto::new(Scope::Body, "{{h}}", 1).unwrap()).unwrap();
        assert_eq!(selected(&a).0, StoryRef::Body);
    }

    /// The shared fixture of the host's tag check (`jump.docx`, `expected.json`) and a file whose
    /// header parts are named against their first reference (`order.docx`).
    const JUMP_DOCX: &[u8] = include_bytes!("../tests/fixtures/tag-jump/jump.docx");
    const JUMP_EXPECTED: &str = include_str!("../tests/fixtures/tag-jump/expected.json");
    const ORDER_DOCX: &[u8] = include_bytes!("../tests/fixtures/tag-jump/order.docx");

    fn open(bytes: &[u8]) -> WordApp {
        let mut a = app("");
        let data = wordcraft_engine::cmd::insert::base64_encode(bytes);
        a.run("file.open", serde_json::json!({"path": "fixture.docx", "data": data})).unwrap();
        a
    }

    /// The text of every paragraph of `scope`, in the order the count runs over them.
    fn paragraphs(a: &WordApp, scope: Scope) -> Vec<String> {
        let doc = &a.session.doc;
        stories(doc, scope)
            .into_iter()
            .flat_map(|st| doc.para_paths(st).into_iter().filter_map(move |p| doc.para(st, &p).map(|p| p.text.clone())))
            .collect()
    }

    fn location(v: &serde_json::Value) -> Goto {
        let scope = Scope::parse(v["story"].as_str().unwrap()).unwrap();
        Goto::new(scope, v["text"].as_str().unwrap(), v["occurrence"].as_u64().unwrap()).unwrap().with_total(v["total"].as_u64()).unwrap()
    }

    /// The paragraph the selection is in.
    fn selected_paragraph(a: &WordApp) -> String {
        a.session.doc.para_at(&a.session.sel.anchor).expect("a paragraph").text.clone()
    }

    #[test]
    fn the_shared_fixture_counts_the_paragraphs_the_host_expects() {
        let a = open(JUMP_DOCX);
        let expected: serde_json::Value = serde_json::from_str(JUMP_EXPECTED).unwrap();
        for (name, scope) in [("body", Scope::Body), ("header", Scope::Header), ("footer", Scope::Footer)] {
            let want: Vec<String> = expected["docx"]["scopes"][name].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
            // The model keeps no mark for a text box's anchor ("Box \u{fffc}", "Mid \u{fffc} end" for the
            // host); it moves no match unless it sits inside one, and then `total` tells (see
            // `a_text_box_inside_a_tag_ends_in_not_found`). Marks are not compared.
            let plain = |v: Vec<String>| v.into_iter().map(|p| p.replace('\u{fffc}', "")).collect::<Vec<_>>();
            let counted_text: Vec<String> = stories(&a.session.doc, scope)
                .into_iter()
                .flat_map(|st| {
                    let doc = &a.session.doc;
                    doc.para_paths(st).into_iter().filter_map(move |p| doc.para(st, &p).map(|p| counted(p).text.into_owned()))
                })
                .collect();
            assert_eq!(plain(counted_text), plain(want), "{name} paragraphs");
        }
    }

    #[test]
    fn the_shared_fixture_locations_point_where_the_host_expects() {
        let mut a = open(JUMP_DOCX);
        let expected: serde_json::Value = serde_json::from_str(JUMP_EXPECTED).unwrap();
        let docx = &expected["docx"];
        let mut places: Vec<(serde_json::Value, Option<String>)> =
            docx["probes"].as_array().unwrap().iter().map(|p| (p["location"].clone(), Some(p["paragraph"].as_str().unwrap().to_string()))).collect();
        for item in docx["errors"].as_array().unwrap().iter().chain(docx["warnings"].as_array().unwrap()) {
            places.push((item["location"].clone(), None));
        }
        assert!(places.len() >= 14);
        for (loc, paragraph) in places {
            let g = location(&loc);
            goto(&mut a, &g).unwrap_or_else(|e| panic!("{loc}: {e:?}"));
            // A mark inside the tag (a bookmark) is part of what is selected.
            assert_eq!(selected(&a).1.replace('\u{fffc}', ""), g.text, "{loc}");
            if let Some(paragraph) = paragraph {
                assert_eq!(selected_paragraph(&a).replace('\u{fffc}', ""), paragraph.replace('\u{fffc}', ""), "{loc}");
            }
        }
        // One past the last match is not found.
        assert_eq!(goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 10).unwrap()), Err(Failure::NotFound));
        // A total that is not the document's is not found either, whatever the occurrence.
        assert_eq!(goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 1).unwrap().with_total(Some(8)).unwrap()), Err(Failure::NotFound));
    }

    #[test]
    fn header_parts_go_by_first_reference_not_by_file_number() {
        let mut a = open(ORDER_DOCX);
        // The first section refers to header2/footer2 and the last to header1/footer1.
        assert_eq!(paragraphs(&a, Scope::Header), ["Header2 {{h}}", "Header1 {{h}}"]);
        assert_eq!(paragraphs(&a, Scope::Footer), ["Footer2 {{h}}", "Footer1 {{h}}"]);
        for (scope, n, want) in [
            (Scope::Header, 1, "Header2 {{h}}"),
            (Scope::Header, 2, "Header1 {{h}}"),
            (Scope::Footer, 1, "Footer2 {{h}}"),
            (Scope::Footer, 2, "Footer1 {{h}}"),
        ] {
            goto(&mut a, &Goto::new(scope, "{{h}}", n).unwrap()).unwrap();
            assert_eq!(selected_paragraph(&a), want);
        }
    }

    /// A field in the middle of a tag: the host counts a field as one U+FFFC too, so the tag is
    /// not there for it either (`{{na\u{fffc}}}`); the fork and the host skip that paragraph alike.
    /// (A drawing with no picture data is dropped by the reader and splits nothing.)
    #[test]
    fn a_tag_split_by_a_field_is_skipped_as_the_host_skips_it() {
        let mut a = open(ORDER_DOCX);
        assert_eq!(paragraphs(&a, Scope::Body), ["Sec1 {{h}}", "End1", "Img {{name}}", "Fld {{na\u{fffc}}}", "Plain {{name}}"]);
        goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 1).unwrap()).unwrap();
        assert_eq!(selected_paragraph(&a), "Img {{name}}");
        goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 2).unwrap()).unwrap();
        assert_eq!(selected_paragraph(&a), "Plain {{name}}");
        assert_eq!(goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 3).unwrap()), Err(Failure::NotFound));
    }

    #[test]
    fn marks_the_host_does_not_count_are_left_out_and_the_whole_tag_is_selected() {
        use wordcraft_doc::{Block, CharProps};
        let mut a = app("x {{name}} y {{name}}");
        if let Some(Block::Para(p)) = a.session.doc.body.get_mut(0).map(std::sync::Arc::make_mut) {
            // A bookmark opens inside the first tag and closes at the end of the paragraph.
            let end = p.text.len();
            p.insert_object(end, InlineObject::BookmarkEnd { name: "b".into() }, &CharProps::default()).unwrap();
            p.insert_object(5, InlineObject::BookmarkStart { name: "b".into() }, &CharProps::default()).unwrap();
        }
        let doc = &a.session.doc;
        assert_eq!(count(doc, Scope::Body, "{{name}}"), 2, "the host counts two tags");
        goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 1).unwrap().with_total(Some(2)).unwrap()).unwrap();
        assert_eq!(selected(&a).1, "{{n\u{fffc}ame}}", "the whole tag, mark inside it included");
        assert_eq!(a.session.sel.anchor.off, 2);
        goto(&mut a, &Goto::new(Scope::Body, "{{name}}", 2).unwrap().with_total(Some(2)).unwrap()).unwrap();
        assert_eq!(selected(&a).1, "{{name}}");
        assert_eq!(a.session.sel.anchor.off, "x {{n\u{fffc}ame}} y ".len());
    }

    #[test]
    fn a_total_that_differs_selects_nothing() {
        let mut a = app("{{a}} {{a}}\n{{a}}");
        goto(&mut a, &Goto::new(Scope::Body, "{{a}}", 3).unwrap().with_total(Some(3)).unwrap()).unwrap();
        let before = a.session.sel.clone();
        for total in [2, 4] {
            let g = Goto::new(Scope::Body, "{{a}}", 1).unwrap().with_total(Some(total)).unwrap();
            assert_eq!(goto(&mut a, &g), Err(Failure::NotFound));
            assert_eq!(a.session.sel, before, "nothing was selected");
        }
        assert_eq!(Goto::new(Scope::Body, "{{a}}", 1).unwrap().with_total(Some(0)), Err(Failure::Error));
    }

    #[test]
    fn a_text_of_1000_utf16_units_is_accepted() {
        // 1000 Korean syllables are 3000 bytes; 500 astral characters are 1000 units and 2000 bytes.
        for text in ["가".repeat(1000), "\u{1F600}".repeat(500)] {
            let mut a = app(&format!("a{text}b"));
            goto(&mut a, &Goto::new(Scope::Body, &text, 1).unwrap().with_total(Some(1)).unwrap()).unwrap();
            assert_eq!(selected(&a).1, text);
        }
    }

    /// The host counts a text box's anchor as a mark, so a tag with an anchor inside it
    /// (`{{na[box]me}}`) is not one for the host. The model keeps no mark for the box, so the fork
    /// sees a tag there and counts one more than the host: `total` differs and nothing is selected.
    #[test]
    fn a_text_box_inside_a_tag_ends_in_not_found() {
        let mut a = app("{{name}} x {{name}}");
        let before = a.session.sel.clone();
        for occurrence in [1, 2] {
            let g = Goto::new(Scope::Body, "{{name}}", occurrence).unwrap().with_total(Some(1)).unwrap();
            assert_eq!(goto(&mut a, &g), Err(Failure::NotFound));
        }
        assert_eq!(a.session.sel, before);
    }

    /// A document of `body`'s paragraphs, with a note of `kind` per text in `notes` (ids grow in that
    /// order); `refs` (indexes into `notes`, one per body reference, at paragraph start) are made in
    /// paragraph `i` for the i-th entry.
    fn with_notes(kind: PartKind, notes: &[&str], refs: &[usize]) -> (WordApp, Vec<u32>) {
        use wordcraft_doc::para::NoteKind;
        let mut a = app(&vec!["body"; refs.len().max(1)].join("\n"));
        let doc = &mut a.session.doc;
        let ids: Vec<u32> =
            notes.iter().map(|t| doc.add_part(kind, vec![wordcraft_doc::para_block(Paragraph::with_text(t, Default::default()))])).collect();
        let nk = if kind == PartKind::Footnote { NoteKind::Footnote } else { NoteKind::Endnote };
        for (i, r) in refs.iter().enumerate() {
            let obj = InlineObject::NoteRef { kind: nk, id: ids[*r], custom: String::new() };
            doc.insert_object(&Pos::body(i, 0), obj, &Default::default()).unwrap();
        }
        (a, ids)
    }

    #[test]
    fn footnotes_go_by_first_reference_not_by_part_id() {
        // Parts 0..3 are made in this order but the body refers to the third first, the first twice,
        // and never to the second.
        let (mut a, ids) = with_notes(PartKind::Footnote, &["n0 {{f}}", "n1 {{f}}", "n2 {{f}} {{f}}"], &[2, 0, 2, 0]);
        assert_eq!(stories(&a.session.doc, Scope::Footnote), [StoryRef::Part(ids[2]), StoryRef::Part(ids[0])]);
        assert_eq!(count(&a.session.doc, Scope::Footnote, "{{f}}"), 3);
        for (n, story, off) in [(1, ids[2], "n2 ".len()), (2, ids[2], "n2 {{f}} ".len()), (3, ids[0], "n0 ".len())] {
            goto(&mut a, &Goto::new(Scope::Footnote, "{{f}}", n).unwrap().with_total(Some(3)).unwrap()).unwrap();
            assert_eq!(selected(&a), (StoryRef::Part(story), "{{f}}".to_string()));
            assert_eq!(a.session.sel.anchor.off, off);
        }
        assert_eq!(goto(&mut a, &Goto::new(Scope::Footnote, "{{f}}", 4).unwrap()), Err(Failure::NotFound));
        // The unreferenced note is not reachable, and the other kind and the body are not mixed in.
        assert_eq!(goto(&mut a, &Goto::new(Scope::Footnote, "n1", 1).unwrap()), Err(Failure::NotFound));
        assert_eq!(count(&a.session.doc, Scope::Endnote, "{{f}}"), 0);
        assert_eq!(count(&a.session.doc, Scope::Body, "{{f}}"), 0);
    }

    #[test]
    fn a_note_total_that_differs_selects_nothing() {
        let (mut a, _) = with_notes(PartKind::Footnote, &["{{f}}", "{{f}}"], &[1, 0]);
        let before = a.session.sel.clone();
        for total in [1, 3] {
            let g = Goto::new(Scope::Footnote, "{{f}}", 1).unwrap().with_total(Some(total)).unwrap();
            assert_eq!(goto(&mut a, &g), Err(Failure::NotFound));
        }
        assert_eq!(a.session.sel, before);
    }

    #[test]
    fn endnotes_are_their_own_story() {
        let (mut a, ids) = with_notes(PartKind::Endnote, &["e0 {{e}}", "e1 {{e}}"], &[1, 0]);
        assert_eq!(Scope::parse("endnote"), Some(Scope::Endnote));
        assert_eq!(Scope::parse("footnote"), Some(Scope::Footnote));
        assert_eq!(count(&a.session.doc, Scope::Footnote, "{{e}}"), 0);
        goto(&mut a, &Goto::new(Scope::Endnote, "{{e}}", 1).unwrap().with_total(Some(2)).unwrap()).unwrap();
        assert_eq!(selected(&a), (StoryRef::Part(ids[1]), "{{e}}".to_string()));
        assert!(a.canvas.scroll_to_caret);
        goto(&mut a, &Goto::new(Scope::Endnote, "{{e}}", 2).unwrap()).unwrap();
        assert_eq!(selected(&a).0, StoryRef::Part(ids[0]));
    }

    /// The shared fixture of the host's note check (`note-jump.docx`, `expected.json`): notes whose
    /// reference order is not their id order, one in a table, one unreferenced, a separator.
    const NOTE_DOCX: &[u8] = include_bytes!("../tests/fixtures/note-jump/note-jump.docx");
    const NOTE_EXPECTED: &str = include_str!("../tests/fixtures/note-jump/expected.json");

    #[test]
    fn the_shared_note_fixture_counts_and_points_where_the_host_expects() {
        let mut a = open(NOTE_DOCX);
        let expected: serde_json::Value = serde_json::from_str(NOTE_EXPECTED).unwrap();
        let docx = &expected["docx"];
        // Marks (the reference, a note's number) are not compared.
        let plain = |v: Vec<String>| v.into_iter().map(|p| p.replace('\u{fffc}', "")).collect::<Vec<_>>();
        for (name, scope) in [("body", Scope::Body), ("footnote", Scope::Footnote), ("endnote", Scope::Endnote)] {
            let want: Vec<String> = docx["scopes"][name].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
            let got: Vec<String> = stories(&a.session.doc, scope)
                .into_iter()
                .flat_map(|st| {
                    let doc = &a.session.doc;
                    doc.para_paths(st).into_iter().filter_map(move |p| doc.para(st, &p).map(|p| counted(p).text.into_owned()))
                })
                .collect();
            assert_eq!(plain(got), plain(want), "{name} paragraphs");
        }
        let mut places: Vec<(serde_json::Value, Option<String>)> =
            docx["probes"].as_array().unwrap().iter().map(|p| (p["location"].clone(), Some(p["paragraph"].as_str().unwrap().to_string()))).collect();
        for item in docx["warnings"].as_array().unwrap() {
            places.push((item["location"].clone(), None));
        }
        assert!(places.iter().any(|(l, _)| l["story"] == "footnote") && places.iter().any(|(l, _)| l["story"] == "endnote"));
        for (loc, paragraph) in places {
            let g = location(&loc);
            goto(&mut a, &g).unwrap_or_else(|e| panic!("{loc}: {e:?}"));
            assert_eq!(selected(&a).1.replace('\u{fffc}', ""), g.text, "{loc}");
            if let Some(paragraph) = paragraph {
                assert_eq!(selected_paragraph(&a).replace('\u{fffc}', ""), paragraph.replace('\u{fffc}', ""), "{loc}");
            }
        }
    }
}
