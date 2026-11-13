//! eframe's web runner calls `preventDefault` only for Cmd/Ctrl + Comma, O, P and S, so the
//! editor's own Find (Cmd/Ctrl+F), Find Next/Previous (Cmd/Ctrl+G, Shift+G, F3) and Replace
//! (Cmd/Ctrl+H) also reach the browser, which opens its find bar (or history) on top of the
//! editor's. The host-locked build keeps those presses from the browser; the app still hears them
//! (the listener never stops propagation). Clipboard keys and plain typing are left alone.

/// Whether the browser's own action for this key press must be cancelled. `code` is
/// `KeyboardEvent.code` (layout independent), `key` is `KeyboardEvent.key`.
pub(crate) fn blocks_browser_default(ctrl: bool, meta: bool, shift: bool, alt: bool, code: &str, key: &str) -> bool {
    if alt {
        return false;
    }
    let letter = |code_name: &str, c: char| code == code_name || key.eq_ignore_ascii_case(c.encode_utf8(&mut [0; 4]));
    if ctrl || meta {
        // F, G and H: Cmd/Ctrl, optionally with Shift (Find Previous). Ctrl+Cmd together is not ours.
        return !(ctrl && meta) && (letter("KeyF", 'f') && !shift || letter("KeyG", 'g') || letter("KeyH", 'h') && !shift);
    }
    // F3 / Shift+F3: the browser's find next / previous.
    code == "F3" || key == "F3"
}

#[cfg(test)]
mod tests {
    use super::blocks_browser_default as blocks;

    #[test]
    fn find_keys_are_blocked_with_ctrl_or_cmd() {
        for (ctrl, meta) in [(true, false), (false, true)] {
            assert!(blocks(ctrl, meta, false, false, "KeyF", "f"), "find");
            assert!(blocks(ctrl, meta, false, false, "KeyG", "g"), "find next");
            assert!(blocks(ctrl, meta, true, false, "KeyG", "G"), "find previous");
            assert!(blocks(ctrl, meta, false, false, "KeyH", "h"), "replace");
        }
    }

    #[test]
    fn layout_independent_and_uppercase() {
        // A non-Latin layout: the key is not "f" but the physical code is.
        assert!(blocks(true, false, false, false, "KeyF", "а"));
        assert!(blocks(true, false, false, false, "", "F"));
    }

    #[test]
    fn f3_is_blocked_only_without_command_keys() {
        assert!(blocks(false, false, false, false, "F3", "F3"));
        assert!(blocks(false, false, true, false, "F3", "F3"));
    }

    #[test]
    fn clipboard_typing_and_other_keys_are_left_alone() {
        for (code, key) in [("KeyC", "c"), ("KeyV", "v"), ("KeyX", "x"), ("KeyZ", "z"), ("KeyA", "a")] {
            assert!(!blocks(true, false, false, false, code, key), "{key}");
            assert!(!blocks(false, true, false, false, code, key), "{key}");
        }
        // Plain and Shift-only letters are typing.
        assert!(!blocks(false, false, false, false, "KeyF", "f"));
        assert!(!blocks(false, false, true, false, "KeyG", "G"));
        // Alt combinations, Shift+Cmd+F and Ctrl+Cmd are not the editor's find keys.
        assert!(!blocks(true, false, false, true, "KeyF", "f"));
        assert!(!blocks(true, false, true, false, "KeyF", "F"));
        assert!(!blocks(true, true, false, false, "KeyF", "f"));
        assert!(!blocks(false, false, false, false, "F5", "F5"));
    }
}
