//! Web paste (temporary tester check): Cmd+V arrives as a key press plus a Paste event (pre-fix)
//! or as the Paste event alone (post-fix). Copied formatting must come back once.

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

fn key_v(pressed: bool) -> egui::Event {
    egui::Event::Key { key: egui::Key::V, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND }
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
fn pre_fix_key_plus_paste_event_duplicates() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.event(key_v(true));
    h.event(egui::Event::Paste("strong".into()));
    h.event(key_v(false));
    h.step();
    println!("PRE-FIX body = {:?}", body(&h));
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

#[test]
fn ime_preedit_then_paste_event() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit { text: "ㅎ".into(), active_range_chars: None }));
    h.step();
    h.event(egui::Event::Paste("strong".into()));
    h.step();
    println!("IME body = {:?} preedit = {:?}", body(&h), h.state().canvas.ime_preedit);
}

#[test]
fn equation_edit_paste_event_types_text_into_equation() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.state_mut().run("insert.equation", json!({})).unwrap();
    h.step();
    println!("MATH opened = {}", h.state().session.math.is_some());
    h.event(egui::Event::Paste("strong".into()));
    h.step();
    println!("MATH post-fix body = {:?} math = {:?}", body(&h), h.state().session.math.is_some());
}

#[test]
fn equation_edit_pre_fix_key_plus_paste() {
    let mut h = harness();
    copy_bold_strong(&mut h);
    h.state_mut().run("insert.equation", json!({})).unwrap();
    h.step();
    h.event(key_v(true));
    h.event(egui::Event::Paste("strong".into()));
    h.event(key_v(false));
    h.step();
    println!("MATH pre-fix body = {:?} math = {:?}", body(&h), h.state().session.math.is_some());
}
