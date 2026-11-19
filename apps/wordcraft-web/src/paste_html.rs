//! The browser clipboard's `text/html` for Paste (#136).
//!
//! eframe's paste handler reads only `text/plain` and queues `Event::Paste(text)`; the app turns
//! that into `edit.paste`, which keeps another program's formatting when `Services::clipboard_html`
//! hands it the HTML (as the desktop app does, #687). `clipboardData` can only be read while the
//! `paste` event is being dispatched, but the app handles the queued event in the next frame, so
//! the HTML waits in a [`Slot`] between the two. The slot is filled by a `paste` listener (same
//! dispatch as eframe's, so the HTML and the queued text are the same paste) and emptied by
//! whichever comes first: `edit.paste` taking it, or a frame that has no paste event (eframe
//! ignored the event, or the page didn't take it), so an old copy is never pasted later by the
//! ribbon's Paste.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// The HTML of each paste event the app has not handled yet, oldest first: one entry per queued
/// paste (`None` when that paste had no HTML), so a held Ctrl+V that queues several pastes in a
/// frame pairs each with its own HTML, as the desktop's clipboard read does.
pub(crate) type Slot = Rc<RefCell<VecDeque<Option<String>>>>;

/// What a `paste` event adds to the queue: nothing when `text/plain` is empty (eframe queues no
/// paste then; HTML alone, a bare picture copied from a page, stays for the picture listener),
/// else an entry holding the HTML if there is any. Size limits and blank checks are the engine's
/// and `system_clipboard::fill`'s.
pub(crate) fn entry(text: &str, html: &str) -> Option<Option<String>> {
    (!text.is_empty()).then(|| (!html.is_empty()).then(|| html.to_string()))
}

/// Record a paste event.
pub(crate) fn push(slot: &Slot, text: &str, html: &str) {
    if let Some(e) = entry(text, html) {
        slot.borrow_mut().push_back(e);
    }
}

/// Called each frame with the events egui is about to run: a frame with no paste event leaves
/// nothing waiting (eframe ignored the event, or the page didn't take it).
pub(crate) fn expire(slot: &Slot, events: &[egui::Event]) {
    if !events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
        slot.borrow_mut().clear();
    }
}

/// What `Services::clipboard_html` returns: the HTML of the oldest waiting paste, once.
pub(crate) fn take(slot: &Slot) -> Option<String> {
    slot.borrow_mut().pop_front().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "<table><tr><td>{{loop EQP}}</td><td></td></tr></table>";

    #[test]
    fn html_is_kept_only_with_text() {
        assert_eq!(entry("{{loop EQP}}\t", TABLE), Some(Some(TABLE.to_string())));
        assert_eq!(entry("text", ""), Some(None));
        assert_eq!(entry("", TABLE), None);
    }

    #[test]
    fn pastes_of_one_frame_each_take_their_own_html() {
        let slot = Slot::default();
        push(&slot, "a", "<b>1</b>");
        push(&slot, "b", "");
        push(&slot, "", "<i>no text: no paste</i>");
        push(&slot, "c", "<b>3</b>");
        assert_eq!(take(&slot).as_deref(), Some("<b>1</b>"));
        assert_eq!(take(&slot), None, "the second paste had no HTML");
        assert_eq!(take(&slot).as_deref(), Some("<b>3</b>"));
        assert_eq!(take(&slot), None);
    }

    #[test]
    fn a_frame_without_a_paste_event_empties_the_slot() {
        let slot = Slot::default();
        push(&slot, "x", TABLE);
        expire(&slot, &[egui::Event::Paste("x".into())]);
        assert_eq!(slot.borrow().len(), 1, "the paste is still to be handled");
        expire(&slot, &[egui::Event::Copy]);
        assert!(slot.borrow().is_empty(), "no paste arrived: drop the old copy");
    }
}
