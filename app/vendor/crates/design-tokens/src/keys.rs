//! Kaliber keyboard rules (plan 094 revision 3, lane K9) in iced 0.14: the same rules as
//! `design/web/kaliber-keys.js`, where iced can express them.
//!
//! Tier 1 here: the primary modifier (Command on macOS, Ctrl elsewhere, never both, as
//! `Modifiers::command()`), shortcut matching ([`Shortcut::matches`]: letters by `key` with the
//! `Key::to_latin` fallback for a non-Latin layout, digits by `physical_key`, punctuation by
//! `modified_key` with Shift ignored, named keys by `key`; Ctrl and Alt together are ignored,
//! because Windows reports AltGr as Ctrl+Alt), the platform labels ([`Shortcut::label`]) and
//! the Escape layer order ([`EscapeStack`]). Tier 2 (no iced 0.14 API): the AltGraph state,
//! `isComposing` and a pending dead key; their rows are in
//! `vault/knowledge/wiki/design-language/components.md`, section "Keyboard".
use iced_core::keyboard::key::{Code, Named, Physical};
use iced_core::keyboard::{Key, Modifiers};

/// The platform whose conventions apply: the modifier and the label format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Command is primary; labels are glyphs (`⇧⌘S`).
    Mac,
    /// Ctrl is primary; labels are words with plus signs (`Ctrl+Shift+S`).
    Windows,
    /// As Windows; the logo key is called Super.
    Linux,
}

impl Platform {
    /// The platform this build runs on.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::Windows
        }
    }
}

/// The key of a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A Latin letter, lower case (`'s'`), matched by `key` with the `to_latin` fallback.
    Letter(char),
    /// A digit 0 to 9, matched by the physical key (top row or numpad).
    Digit(u8),
    /// A punctuation character (`'?'`, `','`, `'+'`), matched by `modified_key`, Shift ignored.
    Punct(char),
    /// A named key (Enter, Escape, F2, Delete, the arrows).
    Named(Named),
}

/// One shortcut: a trigger and its modifiers. `command` is the primary modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub trigger: Trigger,
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
}

/// True for a press the system owns as typing: Ctrl and Alt together (AltGr on Windows).
pub fn ignored(modifiers: Modifiers) -> bool {
    modifiers.control() && modifiers.alt()
}

/// The primary modifier on `platform`: Command (logo) on macOS, Ctrl elsewhere.
pub fn primary(modifiers: Modifiers, platform: Platform) -> bool {
    match platform {
        Platform::Mac => modifiers.logo(),
        _ => modifiers.control(),
    }
}

/// The other one of Ctrl and Command, which must be up for a match.
fn secondary(modifiers: Modifiers, platform: Platform) -> bool {
    match platform {
        Platform::Mac => modifiers.control(),
        _ => modifiers.logo(),
    }
}

const DIGITS: [(Code, Code); 10] = [
    (Code::Digit0, Code::Numpad0),
    (Code::Digit1, Code::Numpad1),
    (Code::Digit2, Code::Numpad2),
    (Code::Digit3, Code::Numpad3),
    (Code::Digit4, Code::Numpad4),
    (Code::Digit5, Code::Numpad5),
    (Code::Digit6, Code::Numpad6),
    (Code::Digit7, Code::Numpad7),
    (Code::Digit8, Code::Numpad8),
    (Code::Digit9, Code::Numpad9),
];

impl Shortcut {
    /// A key with no modifier.
    pub const fn plain(trigger: Trigger) -> Self {
        Shortcut {
            trigger,
            command: false,
            shift: false,
            alt: false,
        }
    }
    /// Primary + key (Ctrl+S, Command+S).
    pub const fn command(trigger: Trigger) -> Self {
        Shortcut {
            trigger,
            command: true,
            shift: false,
            alt: false,
        }
    }
    /// The same shortcut with Shift.
    pub const fn shift(self) -> Self {
        Shortcut {
            shift: true,
            ..self
        }
    }

    /// Does a `KeyPressed` event match, on the platform of this build?
    pub fn matches(
        &self,
        key: &Key,
        modified_key: &Key,
        physical_key: Physical,
        modifiers: Modifiers,
    ) -> bool {
        self.matches_on(
            key,
            modified_key,
            physical_key,
            modifiers,
            Platform::current(),
        )
    }

    /// [`Shortcut::matches`] for an explicit platform (tests, a settings preview).
    pub fn matches_on(
        &self,
        key: &Key,
        modified_key: &Key,
        physical_key: Physical,
        modifiers: Modifiers,
        platform: Platform,
    ) -> bool {
        if ignored(modifiers) || matches!(key, Key::Unidentified) {
            return false;
        }
        if primary(modifiers, platform) != self.command
            || secondary(modifiers, platform)
            || modifiers.alt() != self.alt
        {
            return false;
        }
        let punct = matches!(self.trigger, Trigger::Punct(_));
        if !punct && modifiers.shift() != self.shift {
            return false;
        }
        self.trigger_matches(key, modified_key, physical_key)
    }

    fn trigger_matches(&self, key: &Key, modified_key: &Key, physical_key: Physical) -> bool {
        match self.trigger {
            Trigger::Letter(c) => {
                key.to_latin(physical_key).map(|l| l.to_ascii_lowercase()) == Some(c)
            }
            Trigger::Digit(d) => DIGITS.get(usize::from(d)).is_some_and(|(top, pad)| {
                physical_key == Physical::Code(*top) || physical_key == Physical::Code(*pad)
            }),
            Trigger::Punct(c) => {
                matches!(modified_key.as_ref(), Key::Character(s) if s.chars().eq(std::iter::once(c)))
            }
            Trigger::Named(n) => matches!(key, Key::Named(k) if *k == n),
        }
    }

    /// The label: `Ctrl+Shift+S` on Windows and Linux, `⇧⌘S` on macOS (Control, Option, Shift,
    /// Command), and `⌘?` for punctuation (the upper character, no Shift).
    pub fn label(&self, platform: Platform) -> String {
        let shift = self.shift && !matches!(self.trigger, Trigger::Punct(_));
        let key = key_text(self.trigger, platform);
        if platform == Platform::Mac {
            let mut out = String::new();
            if self.alt {
                out.push('⌥');
            }
            if shift {
                out.push('⇧');
            }
            if self.command {
                out.push('⌘');
            }
            return out + &key;
        }
        let mut parts: Vec<String> = Vec::new();
        if self.command {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if shift {
            parts.push("Shift".into());
        }
        parts.push(key);
        parts.join("+")
    }
}

fn key_text(trigger: Trigger, platform: Platform) -> String {
    let mac = platform == Platform::Mac;
    match trigger {
        Trigger::Letter(c) => c.to_ascii_uppercase().to_string(),
        Trigger::Digit(d) => d.to_string(),
        Trigger::Punct('+') if !mac => "Plus".into(),
        Trigger::Punct('-') if !mac => "Minus".into(),
        Trigger::Punct(c) => c.to_string(),
        Trigger::Named(n) => named_text(n, mac),
    }
}

fn named_text(n: Named, mac: bool) -> String {
    let (pc, glyph) = match n {
        Named::Escape => ("Esc", "⎋"),
        Named::Delete => ("Del", "⌦"),
        Named::Backspace => ("Backspace", "⌫"),
        Named::Enter => ("Enter", "↩"),
        Named::Tab => ("Tab", "⇥"),
        Named::Space => ("Space", "Space"),
        Named::ArrowUp => ("Up", "↑"),
        Named::ArrowDown => ("Down", "↓"),
        Named::ArrowLeft => ("Left", "←"),
        Named::ArrowRight => ("Right", "→"),
        Named::PageUp => ("PgUp", "⇞"),
        Named::PageDown => ("PgDn", "⇟"),
        Named::Home => ("Home", "↖"),
        Named::End => ("End", "↘"),
        other => return format!("{other:?}"),
    };
    (if mac { glyph } else { pc }).into()
}

/// The Escape layers, from the top. One press acts on the top layer only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Drag = 1,
    Transient = 2,
    Popup = 3,
    Edit = 4,
    Place = 5,
    Search = 6,
    Selection = 7,
    Dialog = 8,
    Fullscreen = 9,
}

/// The open layers of one window. While a dialog is open, only layers opened after it count,
/// so a selection behind a dialog stays when Escape closes the dialog. With nothing open,
/// [`EscapeStack::top`] is `None` and Escape does nothing (never closes the window).
#[derive(Debug, Clone, Default)]
pub struct EscapeStack<Id> {
    layers: Vec<(Layer, Id)>,
}

impl<Id: PartialEq + Clone> EscapeStack<Id> {
    pub fn new() -> Self {
        EscapeStack { layers: Vec::new() }
    }
    /// Open a layer (again: it moves to the top of its rank).
    pub fn push(&mut self, layer: Layer, id: Id) {
        self.pop(&id);
        self.layers.push((layer, id));
    }
    /// Close a layer.
    pub fn pop(&mut self, id: &Id) {
        self.layers.retain(|(_, open)| open != id);
    }
    /// The layer one Escape press acts on: the lowest rank, the newest on a tie, inside the
    /// newest dialog.
    pub fn top(&self) -> Option<(Layer, Id)> {
        let start = self
            .layers
            .iter()
            .rposition(|(layer, _)| *layer == Layer::Dialog)
            .unwrap_or(0);
        self.layers[start..]
            .iter()
            .enumerate()
            .min_by_key(|(index, (layer, _))| (*layer, std::cmp::Reverse(*index)))
            .map(|(_, (layer, id))| (*layer, id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::keyboard::key::Code;

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }
    fn code(c: Code) -> Physical {
        Physical::Code(c)
    }
    const S: Shortcut = Shortcut::command(Trigger::Letter('s'));

    #[test]
    fn primary_is_ctrl_on_windows_and_command_on_mac_never_both() {
        let k = ch("s");
        assert!(S.matches_on(&k, &k, code(Code::KeyS), Modifiers::CTRL, Platform::Windows));
        assert!(!S.matches_on(&k, &k, code(Code::KeyS), Modifiers::LOGO, Platform::Windows));
        assert!(!S.matches_on(
            &k,
            &k,
            code(Code::KeyS),
            Modifiers::CTRL | Modifiers::LOGO,
            Platform::Windows
        ));
        assert!(S.matches_on(&k, &k, code(Code::KeyS), Modifiers::LOGO, Platform::Mac));
        assert!(!S.matches_on(&k, &k, code(Code::KeyS), Modifiers::CTRL, Platform::Mac));
    }

    #[test]
    fn ctrl_and_alt_together_are_altgr_and_never_match() {
        let k = ch("v");
        let paste = Shortcut::command(Trigger::Letter('v'));
        assert!(!paste.matches_on(
            &k,
            &ch("@"),
            code(Code::KeyV),
            Modifiers::CTRL | Modifiers::ALT,
            Platform::Windows
        ));
        assert!(!Shortcut { alt: true, ..paste }.matches_on(
            &k,
            &k,
            code(Code::KeyV),
            Modifiers::CTRL | Modifiers::ALT,
            Platform::Windows
        ));
    }

    #[test]
    fn a_slovak_top_row_digit_matches_by_physical_key() {
        let zero = Shortcut::command(Trigger::Digit(0));
        assert!(zero.matches_on(
            &ch("é"),
            &ch("é"),
            code(Code::Digit0),
            Modifiers::CTRL,
            Platform::Windows
        ));
        assert!(zero.matches_on(
            &ch("0"),
            &ch("0"),
            code(Code::Numpad0),
            Modifiers::CTRL,
            Platform::Windows
        ));
        assert!(!zero.matches_on(
            &ch("0"),
            &ch("0"),
            code(Code::KeyP),
            Modifiers::CTRL,
            Platform::Windows
        ));
    }

    #[test]
    fn letters_match_the_key_on_qwertz_and_fall_back_for_cyrillic() {
        let undo = Shortcut::command(Trigger::Letter('z'));
        let redo = Shortcut::command(Trigger::Letter('y'));
        assert!(undo.matches_on(
            &ch("z"),
            &ch("z"),
            code(Code::KeyY),
            Modifiers::CTRL,
            Platform::Windows
        ));
        assert!(!redo.matches_on(
            &ch("z"),
            &ch("z"),
            code(Code::KeyY),
            Modifiers::CTRL,
            Platform::Windows
        ));
        assert!(undo.matches_on(
            &ch("я"),
            &ch("я"),
            code(Code::KeyZ),
            Modifiers::CTRL,
            Platform::Windows
        ));
        assert!(undo.shift().matches_on(
            &ch("z"),
            &ch("Z"),
            code(Code::KeyY),
            Modifiers::CTRL | Modifiers::SHIFT,
            Platform::Windows
        ));
        assert!(!undo.matches_on(
            &ch("z"),
            &ch("Z"),
            code(Code::KeyY),
            Modifiers::CTRL | Modifiers::SHIFT,
            Platform::Windows
        ));
    }

    #[test]
    fn punctuation_uses_the_modified_key_and_ignores_shift() {
        let help = Shortcut::plain(Trigger::Punct('?'));
        assert!(help.matches_on(
            &ch(","),
            &ch("?"),
            code(Code::Comma),
            Modifiers::SHIFT,
            Platform::Windows
        ));
        assert!(!help.matches_on(
            &ch(","),
            &ch(","),
            code(Code::Comma),
            Modifiers::empty(),
            Platform::Windows
        ));
        assert!(!help.matches_on(
            &Key::Unidentified,
            &ch("?"),
            code(Code::Comma),
            Modifiers::SHIFT,
            Platform::Windows
        ));
    }

    #[test]
    fn named_keys_match_by_key() {
        let rename = Shortcut::plain(Trigger::Named(Named::F2));
        assert!(rename.matches_on(
            &Key::Named(Named::F2),
            &Key::Named(Named::F2),
            code(Code::F2),
            Modifiers::empty(),
            Platform::Windows
        ));
        assert!(!rename.matches_on(
            &Key::Named(Named::F3),
            &Key::Named(Named::F3),
            code(Code::F3),
            Modifiers::empty(),
            Platform::Windows
        ));
    }

    #[test]
    fn labels_follow_the_platform() {
        assert_eq!(S.shift().label(Platform::Windows), "Ctrl+Shift+S");
        assert_eq!(S.shift().label(Platform::Mac), "⇧⌘S");
        assert_eq!(
            Shortcut::command(Trigger::Punct('?'))
                .shift()
                .label(Platform::Mac),
            "⌘?"
        );
        assert_eq!(
            Shortcut::command(Trigger::Punct('+')).label(Platform::Linux),
            "Ctrl+Plus"
        );
        assert_eq!(
            Shortcut::plain(Trigger::Named(Named::Escape)).label(Platform::Windows),
            "Esc"
        );
        assert_eq!(
            Shortcut {
                alt: true,
                ..S.shift()
            }
            .label(Platform::Mac),
            "⌥⇧⌘S"
        );
        assert_eq!(
            Shortcut::command(Trigger::Digit(1)).label(Platform::Windows),
            "Ctrl+1"
        );
    }

    #[test]
    fn escape_acts_on_one_layer_in_rank_order_inside_the_newest_dialog() {
        let mut stack = EscapeStack::new();
        assert_eq!(stack.top(), None);
        stack.push(Layer::Selection, "table");
        stack.push(Layer::Search, "filter");
        assert_eq!(stack.top(), Some((Layer::Search, "filter")));
        stack.push(Layer::Dialog, "edit");
        stack.push(Layer::Popup, "category");
        stack.push(Layer::Transient, "tip");
        assert_eq!(stack.top(), Some((Layer::Transient, "tip")));
        stack.pop(&"tip");
        assert_eq!(stack.top(), Some((Layer::Popup, "category")));
        stack.pop(&"category");
        assert_eq!(stack.top(), Some((Layer::Dialog, "edit")));
        stack.pop(&"edit");
        assert_eq!(stack.top(), Some((Layer::Search, "filter")));
        stack.pop(&"filter");
        assert_eq!(stack.top(), Some((Layer::Selection, "table")));
    }
}
