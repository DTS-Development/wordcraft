//! Where the other people in a shared document are: the caret and selection each one has, as the
//! parent page sends them (`collab-cursors`, region `doc`, pos `{anchor, head}` in the engine's own
//! `Pos` form). Drawn over the pages (a shaded selection, a caret bar and a name tag). Draw-only:
//! nothing here takes input.
//!
//! The parent supplies the name and colour (the same ones as its participant list); this side only
//! checks and draws them. A malformed or out-of-date entry is skipped, never an error. Only the
//! body is drawn: a place in a header, footer, note or text box is left out for now.

use egui::{Align2, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, pos2, vec2};
use serde_json::Value;
use wordcraft_doc::{Document, Pos, StoryRef};
use wordcraft_layout::DocLayout;

use crate::theme;

/// Most people drawn: a longer list is cut (a hostile or broken parent can't make a frame slow).
pub const MAX_CURSORS: usize = 200;
/// Longest name drawn, in characters.
pub const MAX_NAME_CHARS: usize = 40;
/// Name tags stacked on one spot before "+N".
const MAX_TAGS: usize = 4;
/// Grey for a colour that isn't `#rrggbb`.
const FALLBACK: [u8; 3] = [128, 128, 128];

/// One other connection's place in the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteCursor {
    pub conn: u64,
    pub name: String,
    pub color: [u8; 3],
    /// Only looking (role `view`): the tag has a dashed border.
    pub view: bool,
    pub anchor: Pos,
    pub head: Pos,
}

/// The `pos` this editor reports for its own selection.
pub fn pos_json(anchor: &Pos, head: &Pos) -> Value {
    serde_json::json!({ "anchor": anchor, "head": head })
}

/// `#rrggbb` as bytes.
fn parse_color(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// The people in a `collab-cursors` message that have a place in the document (region `doc` and a
/// readable `pos`), in order. Everything else is left out; the rest are kept.
pub fn parse(m: &Value) -> Vec<RemoteCursor> {
    let Some(list) = m.get("cursors").and_then(Value::as_array) else { return vec![] };
    list.iter().take(MAX_CURSORS).filter_map(parse_one).collect()
}

fn parse_one(c: &Value) -> Option<RemoteCursor> {
    if c.get("region")?.as_str()? != "doc" {
        return None;
    }
    let conn = c.get("conn")?.as_u64()?;
    let pos = c.get("pos")?;
    let anchor: Pos = serde_json::from_value(pos.get("anchor")?.clone()).ok()?;
    let head: Pos = serde_json::from_value(pos.get("head")?.clone()).ok()?;
    let name: String = c.get("name").and_then(Value::as_str).unwrap_or_default().chars().filter(|ch| !ch.is_control()).take(MAX_NAME_CHARS).collect();
    let color = c.get("color").and_then(Value::as_str).and_then(parse_color).unwrap_or(FALLBACK);
    let view = c.get("role").and_then(Value::as_str) == Some("view");
    Some(RemoteCursor { conn, name, color, view, anchor, head })
}

/// Whether `p` is a place this document has: a body paragraph and an offset inside it.
fn valid(doc: &Document, p: &Pos) -> bool {
    p.story == StoryRef::Body && doc.para_at(p).is_some_and(|para| para.clamp(p.off) == p.off)
}

/// What to draw for one person, in page coordinates (points): the selection's rectangles per page
/// and the caret bar `(page, x, top, height)`. `None` when either end isn't in this document (the
/// person's document is a revision ahead or behind ours) or the caret has no place in the layout.
pub struct Figure {
    pub rects: Vec<(usize, wordcraft_geom::Rect)>,
    pub caret: (usize, f32, f32, f32),
}

pub fn figure(doc: &Document, layout: &DocLayout, c: &RemoteCursor, page_hint: usize) -> Option<Figure> {
    if !valid(doc, &c.anchor) || !valid(doc, &c.head) {
        return None;
    }
    let caret = layout.caret_on(&c.head, page_hint)?;
    let rects = if c.anchor == c.head { vec![] } else { layout.selection_rects(doc, &c.anchor, &c.head, page_hint) };
    Some(Figure { rects, caret: (caret.page, caret.x, caret.top, caret.height.max(4.0)) })
}

fn color(c: &RemoteCursor, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.color[0], c.color[1], c.color[2], alpha)
}

/// Black or white, whichever reads on `c`.
fn on(c: [u8; 3]) -> Color32 {
    let l = 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
    if l > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// A name tag sitting on `at` (the caret's top). `row` stacks tags that share a spot; `below`
/// stacks them downward from `at` instead (the caret is at the page's top edge). The tag is kept
/// inside `page` sideways.
fn tag(p: &Painter, at: Pos2, c: &RemoteCursor, row: usize, below: bool, page: Rect) {
    let text = if c.name.is_empty() { "?" } else { c.name.as_str() };
    let galley = p.layout_no_wrap(text.to_string(), theme::regular(11.0), on(c.color));
    let size = galley.size() + vec2(8.0, 4.0);
    let x = at.x.min(page.max.x - size.x).max(page.min.x);
    let y = if below { at.y + size.y * row as f32 } else { at.y - size.y * (row as f32 + 1.0) };
    let r = Rect::from_min_size(pos2(x, y), size);
    p.rect_filled(r, CornerRadius::same(3), color(c, 255));
    if c.view {
        // A viewer's tag has a dashed border (no icon): it is told apart from an editor's by shape.
        let b = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        p.extend(egui::Shape::dashed_line(&b, Stroke::new(1.0, on(c.color)), 3.0, 2.0));
    }
    p.galley(r.min + vec2(4.0, 2.0), galley, on(c.color));
}

/// Draw the others' selections and carets. `pages` are the pages' screen rectangles, `scale_of`
/// the screen size of a point on each. Anything outside `clip` is skipped.
pub fn paint(
    p: &Painter,
    clip: Rect,
    all: &[RemoteCursor],
    doc: &Document,
    layout: &DocLayout,
    page_hint: usize,
    pages: &[Rect],
    scale_of: impl Fn(usize, Rect) -> f32,
) {
    // Tags on one spot stack: counted by (page, x, y) of the caret in whole screen pixels.
    let mut stacked: Vec<((usize, i32, i32), usize)> = vec![];
    for c in all {
        let Some(f) = figure(doc, layout, c, page_hint) else { continue };
        for (pi, r) in &f.rects {
            let Some(pr) = pages.get(*pi) else { continue };
            let s = scale_of(*pi, *pr);
            let sr = Rect::from_min_size(pos2(pr.min.x + r.x * s, pr.min.y + r.y * s), vec2(r.w * s, r.h * s));
            if sr.intersects(clip) {
                p.rect_filled(sr, 0.0, color(c, 40));
            }
        }
        let (pi, x, top, h) = f.caret;
        let Some(pr) = pages.get(pi) else { continue };
        let s = scale_of(pi, *pr);
        let (sx, sy) = ((pr.min.x + x * s).round(), pr.min.y + top * s);
        let bar = Rect::from_min_max(pos2(sx - 1.0, sy), pos2(sx + 1.0, sy + h * s));
        if !bar.intersects(clip) {
            continue;
        }
        p.rect_filled(bar, 0.0, color(c, 255));
        let key = (pi, sx as i32, sy as i32);
        let row = stacked.iter().find(|(k, _)| *k == key).map_or(0, |(_, n)| *n);
        let below = sy - 20.0 < pr.min.y;
        if row < MAX_TAGS {
            tag(p, pos2(sx - 1.0, if below { bar.max.y } else { sy }), c, row, below, *pr);
        }
        match stacked.iter_mut().find(|(k, _)| *k == key) {
            Some(s) => s.1 += 1,
            None => stacked.push((key, 1)),
        }
    }
    for ((_, x, y), n) in stacked {
        if n > MAX_TAGS {
            p.text(pos2(x as f32, y as f32 - 16.0 * (MAX_TAGS as f32 + 1.0)), Align2::LEFT_BOTTOM, format!("+{}", n - MAX_TAGS), theme::regular(11.0), Color32::GRAY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wordcraft_doc::Path;

    fn pos(off: usize) -> Value {
        json!({"story": "body", "path": [0], "off": off})
    }

    fn entry(conn: u64, region: &str, pos: Value) -> Value {
        json!({"conn": conn, "user": "u", "name": "Kim", "color": "#ff8800", "role": "edit", "region": region, "pos": pos})
    }

    #[test]
    fn reads_doc_positions_and_skips_the_rest() {
        let m = json!({"cursors": [
            entry(1, "doc", json!({"anchor": pos(1), "head": pos(3)})),
            entry(2, "sheet:0", json!({"anchor": pos(1), "head": pos(1)})),
            entry(3, "doc", json!({"anchor": pos(1)})),
            entry(4, "doc", json!({"anchor": "x", "head": 5})),
            json!({"conn": 5, "region": "doc"}),
            json!("junk"),
        ]});
        let got = parse(&m);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].conn, 1);
        assert_eq!((got[0].anchor.off, got[0].head.off), (1, 3));
        assert_eq!(got[0].head.path, Path(vec![0]));
        assert_eq!(got[0].color, [255, 136, 0]);
        assert!(!got[0].view);
    }

    #[test]
    fn guards_the_size_name_and_colour() {
        let many: Vec<Value> = (0..300).map(|i| entry(i, "doc", json!({"anchor": pos(0), "head": pos(0)}))).collect();
        assert_eq!(parse(&json!({"cursors": many})).len(), MAX_CURSORS);
        let long = json!({"conn": 1, "name": "x".repeat(500), "color": "red", "role": "view", "region": "doc", "pos": {"anchor": pos(0), "head": pos(0)}});
        let got = parse(&json!({"cursors": [long]}));
        assert_eq!(got[0].name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(got[0].color, FALLBACK);
        assert!(got[0].view);
        // Not a list: nothing, never a panic.
        assert!(parse(&json!({"cursors": 5})).is_empty());
        assert!(parse(&json!({})).is_empty());
        // Control characters can't ride in a name.
        let ctl = json!({"conn": 1, "name": "a\u{0}b\nc", "region": "doc", "pos": {"anchor": pos(0), "head": pos(0)}});
        assert_eq!(parse(&json!({"cursors": [ctl]}))[0].name, "abc");
    }

    #[test]
    fn colours_parse_only_as_hex() {
        assert_eq!(parse_color("#00ff7F"), Some([0, 255, 127]));
        for bad in ["00ff7f", "#fff", "#gggggg", "#00ff7f00", ""] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    fn cursor(anchor: Pos, head: Pos) -> RemoteCursor {
        RemoteCursor { conn: 1, name: "A".into(), color: [1, 2, 3], view: false, anchor, head }
    }

    #[test]
    fn the_own_place_is_the_engines_pos_form() {
        let p = Pos::new(StoryRef::Body, Path(vec![2]), 5);
        assert_eq!(pos_json(&p, &p), json!({"anchor": {"story": "body", "path": [2], "off": 5}, "head": {"story": "body", "path": [2], "off": 5}}));
        // What goes out reads back in.
        let back: Pos = serde_json::from_value(pos_json(&p, &p)["head"].clone()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn a_place_the_document_lacks_has_no_figure() {
        let mut session = wordcraft_engine::Session::new(Document::new());
        session.run("text.insert", &json!({"text": "hello world"})).unwrap();
        let layout = session.layout();
        let at = |path: Vec<u32>, off| Pos::new(StoryRef::Body, Path(path), off);
        let f = figure(&session.doc, &layout, &cursor(at(vec![0], 1), at(vec![0], 5)), 0).expect("a place in the text");
        assert!(!f.rects.is_empty(), "a selection shades its text");
        let caret = figure(&session.doc, &layout, &cursor(at(vec![0], 2), at(vec![0], 2)), 0).expect("a caret");
        assert!(caret.rects.is_empty());
        for bad in [
            cursor(at(vec![9], 0), at(vec![9], 0)),
            cursor(at(vec![0], 0), at(vec![0], 999)),
            cursor(at(vec![0, 1, 2, 3], 0), at(vec![0], 0)),
            cursor(Pos::new(StoryRef::Part(7), Path(vec![0]), 0), at(vec![0], 0)),
            cursor(at(vec![0], 0), Pos::new(StoryRef::Part(0), Path(vec![0]), 0)),
        ] {
            assert!(figure(&session.doc, &layout, &bad, 0).is_none());
        }
    }
}
