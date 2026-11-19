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
use std::rc::Rc;

/// The HTML of the paste event the app has not handled yet.
pub(crate) type Slot = Rc<RefCell<Option<String>>>;

/// The HTML to keep for a paste event whose `text/plain` is `text`. eframe queues a paste only
/// when it has text, so HTML alone (a bare picture copied from a page) stays for the picture
/// listener. Size limits and blank checks are the engine's and `system_clipboard::fill`'s.
pub(crate) fn capture(text: &str, html: &str) -> Option<String> {
    (!text.is_empty() && !html.is_empty()).then(|| html.to_string())
}

/// Called each frame with the events egui is about to run: a paste that isn't among them
/// leaves nothing waiting.
pub(crate) fn expire(slot: &Slot, events: &[egui::Event]) {
    if !events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
        slot.borrow_mut().take();
    }
}

/// What `Services::clipboard_html` returns: the waiting HTML, once.
pub(crate) fn take(slot: &Slot) -> Option<String> {
    slot.borrow_mut().take()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "<table><tr><td>{{loop EQP}}</td><td></td></tr></table>";

    #[test]
    fn html_is_kept_only_with_text() {
        assert_eq!(capture("{{loop EQP}}\t", TABLE).as_deref(), Some(TABLE));
        assert_eq!(capture("", TABLE), None);
        assert_eq!(capture("text", ""), None);
    }

    #[test]
    fn taken_once() {
        let slot = Slot::default();
        *slot.borrow_mut() = Some(TABLE.into());
        assert_eq!(take(&slot).as_deref(), Some(TABLE));
        assert_eq!(take(&slot), None);
    }

    #[test]
    fn a_frame_without_a_paste_event_empties_the_slot() {
        let slot = Slot::default();
        *slot.borrow_mut() = Some(TABLE.into());
        expire(&slot, &[egui::Event::Paste("x".into())]);
        assert!(slot.borrow().is_some(), "the paste is still to be handled");
        expire(&slot, &[egui::Event::Copy]);
        assert!(slot.borrow().is_none(), "no paste arrived: drop the old copy");
    }
}
