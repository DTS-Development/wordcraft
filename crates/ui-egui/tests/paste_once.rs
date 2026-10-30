//! Web paste: the browser shell drops the Cmd+V key press (`wordcraft-web` clipboard_keys), so the
//! app sees the Paste event alone. Pasting the same text as the app's own copy keeps its formatting once.

use serde_json::json;
use wordcraft_engine::Session;
use wordcraft_ui_egui::{Services, WordApp};

fn harness() -> egui_kittest::Harness<'static, WordApp> {
    let app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("plain strong")), Services::default());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    for _ in 0..6 {
        h.step();
    }
    h
}

fn body(h: &egui_kittest::Harness<'static, WordApp>) -> String {
    h.state().session.doc.plain_text(wordcraft_doc::StoryRef::Body)
}

fn bold_at(h: &egui_kittest::Harness<'static, WordApp>, off: usize) -> bool {
    let a = h.state();
    let focus = a.session.sel.focus.clone();
    a.session.doc.para_at(&focus).and_then(|p| p.props_of_char(off).bold).unwrap_or(false)
}

/// Bold "strong", copy it, collapse to the end.
fn copy_bold_strong(h: &mut egui_kittest::Harness<'static, WordApp>) {
    h.state_mut().run("select.text", json!({"text": "strong"})).unwrap();
    h.state_mut().run("format.bold", json!({})).unwrap();
    h.state_mut().run("edit.copy", json!({})).unwrap();
    h.state_mut().run("select.collapse", json!({"end": true})).unwrap();
    h.step();
}

#[test]
fn post_fix_paste_event_alone_keeps_formatting_once() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.event(egui::Event::Paste("strong".into()));
    h.step();
    assert_eq!(body(&h), "plain strongstrong");
    assert!(bold_at(&h, 12), "pasted copy keeps bold");
    assert!(bold_at(&h, 6), "original keeps its bold");
}

#[test]
fn paste_text_differing_from_internal_clipboard_is_plain() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.event(egui::Event::Paste("outside".into()));
    h.step();
    assert_eq!(body(&h), "plain strongoutside");
}

#[test]
fn empty_paste_event_changes_nothing() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    let before = body(&h);
    h.event(egui::Event::Paste(String::new()));
    h.step();
    assert_eq!(body(&h), before);
}
