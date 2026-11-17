//! Key mapping for the UWP backend: `Windows.System.VirtualKey` codes (the same numbers as Win32's
//! VK codes) to winit's logical keys, and set-1 scancodes to physical keys. The scancode table is
//! winit's Windows backend's, copied because that module only builds for desktop Windows.

use crate::keyboard::{KeyCode, KeyLocation, NamedKey, NativeKeyCode, PhysicalKey};

/// The physical key of a scancode, `extended` for the keys whose scancode starts with `E0`.
pub(super) fn physical_key(scancode: u32, extended: bool) -> PhysicalKey {
    scancode_to_physicalkey(if extended {
        0xe000 | scancode
    } else {
        scancode
    })
}

/// Gamepad and navigation keys (`VirtualKey::GamepadA` to `NavigationCancel`). The console sends
/// them for a controller's buttons too; the game reads controllers itself, so they are dropped
/// rather than reported twice.
pub(super) fn is_gamepad(vk: i32) -> bool {
    (0x88..=0x8f).contains(&vk) || (0xc3..=0xda).contains(&vk)
}

/// The named key of a virtual key, for the keys that have one.
pub(super) fn named_key(vk: i32) -> Option<NamedKey> {
    Some(match vk {
        0x08 => NamedKey::Backspace,
        0x09 => NamedKey::Tab,
        0x0c => NamedKey::Clear,
        0x0d => NamedKey::Enter,
        0x10 | 0xa0 | 0xa1 => NamedKey::Shift,
        0x11 | 0xa2 | 0xa3 => NamedKey::Control,
        0x12 | 0xa4 | 0xa5 => NamedKey::Alt,
        0x13 => NamedKey::Pause,
        0x14 => NamedKey::CapsLock,
        0x1b => NamedKey::Escape,
        0x20 => NamedKey::Space,
        0x21 => NamedKey::PageUp,
        0x22 => NamedKey::PageDown,
        0x23 => NamedKey::End,
        0x24 => NamedKey::Home,
        0x25 => NamedKey::ArrowLeft,
        0x26 => NamedKey::ArrowUp,
        0x27 => NamedKey::ArrowRight,
        0x28 => NamedKey::ArrowDown,
        0x2c => NamedKey::PrintScreen,
        0x2d => NamedKey::Insert,
        0x2e => NamedKey::Delete,
        0x2f => NamedKey::Help,
        0x5b | 0x5c => NamedKey::Super,
        0x5d => NamedKey::ContextMenu,
        0x70 => NamedKey::F1,
        0x71 => NamedKey::F2,
        0x72 => NamedKey::F3,
        0x73 => NamedKey::F4,
        0x74 => NamedKey::F5,
        0x75 => NamedKey::F6,
        0x76 => NamedKey::F7,
        0x77 => NamedKey::F8,
        0x78 => NamedKey::F9,
        0x79 => NamedKey::F10,
        0x7a => NamedKey::F11,
        0x7b => NamedKey::F12,
        0x7c => NamedKey::F13,
        0x7d => NamedKey::F14,
        0x7e => NamedKey::F15,
        0x7f => NamedKey::F16,
        0x80 => NamedKey::F17,
        0x81 => NamedKey::F18,
        0x82 => NamedKey::F19,
        0x83 => NamedKey::F20,
        0x84 => NamedKey::F21,
        0x85 => NamedKey::F22,
        0x86 => NamedKey::F23,
        0x87 => NamedKey::F24,
        0x90 => NamedKey::NumLock,
        0x91 => NamedKey::ScrollLock,
        _ => return None,
    })
}

/// The character a key writes without modifiers, where the virtual key alone says it (letters,
/// digits, the numpad's digits and operators). Punctuation depends on the layout: its character
/// comes from the `CharacterReceived` event that follows the key.
pub(super) fn base_char(vk: i32) -> Option<char> {
    match vk {
        0x30..=0x39 => char::from_u32(vk as u32),
        0x41..=0x5a => char::from_u32(vk as u32 + 0x20),
        0x60..=0x69 => char::from_u32(vk as u32 - 0x60 + 0x30),
        0x6a => Some('*'),
        0x6b => Some('+'),
        0x6d => Some('-'),
        0x6e => Some('.'),
        0x6f => Some('/'),
        _ => None,
    }
}

/// Where the key sits: left or right for modifiers, numpad for its keys.
pub(super) fn location(physical: PhysicalKey) -> KeyLocation {
    let PhysicalKey::Code(code) = physical else {
        return KeyLocation::Standard;
    };
    match code {
        KeyCode::ShiftLeft | KeyCode::ControlLeft | KeyCode::AltLeft | KeyCode::SuperLeft => {
            KeyLocation::Left
        }
        KeyCode::ShiftRight | KeyCode::ControlRight | KeyCode::AltRight | KeyCode::SuperRight => {
            KeyLocation::Right
        }
        KeyCode::Numpad0
        | KeyCode::Numpad1
        | KeyCode::Numpad2
        | KeyCode::Numpad3
        | KeyCode::Numpad4
        | KeyCode::Numpad5
        | KeyCode::Numpad6
        | KeyCode::Numpad7
        | KeyCode::Numpad8
        | KeyCode::Numpad9
        | KeyCode::NumpadAdd
        | KeyCode::NumpadSubtract
        | KeyCode::NumpadMultiply
        | KeyCode::NumpadDivide
        | KeyCode::NumpadDecimal
        | KeyCode::NumpadEnter
        | KeyCode::NumLock => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

fn scancode_to_physicalkey(scancode: u32) -> PhysicalKey {
    // See: https://www.win.tue.nl/~aeb/linux/kbd/scancodes-1.html
    // and: https://www.w3.org/TR/uievents-code/
    // and: The widget/NativeKeyToDOMCodeName.h file in the firefox source

    PhysicalKey::Code(match scancode {
        0x0029 => KeyCode::Backquote,
        0x002b => KeyCode::Backslash,
        0x000e => KeyCode::Backspace,
        0x001a => KeyCode::BracketLeft,
        0x001b => KeyCode::BracketRight,
        0x0033 => KeyCode::Comma,
        0x000b => KeyCode::Digit0,
        0x0002 => KeyCode::Digit1,
        0x0003 => KeyCode::Digit2,
        0x0004 => KeyCode::Digit3,
        0x0005 => KeyCode::Digit4,
        0x0006 => KeyCode::Digit5,
        0x0007 => KeyCode::Digit6,
        0x0008 => KeyCode::Digit7,
        0x0009 => KeyCode::Digit8,
        0x000a => KeyCode::Digit9,
        0x000d => KeyCode::Equal,
        0x0056 => KeyCode::IntlBackslash,
        0x0073 => KeyCode::IntlRo,
        0x007d => KeyCode::IntlYen,
        0x001e => KeyCode::KeyA,
        0x0030 => KeyCode::KeyB,
        0x002e => KeyCode::KeyC,
        0x0020 => KeyCode::KeyD,
        0x0012 => KeyCode::KeyE,
        0x0021 => KeyCode::KeyF,
        0x0022 => KeyCode::KeyG,
        0x0023 => KeyCode::KeyH,
        0x0017 => KeyCode::KeyI,
        0x0024 => KeyCode::KeyJ,
        0x0025 => KeyCode::KeyK,
        0x0026 => KeyCode::KeyL,
        0x0032 => KeyCode::KeyM,
        0x0031 => KeyCode::KeyN,
        0x0018 => KeyCode::KeyO,
        0x0019 => KeyCode::KeyP,
        0x0010 => KeyCode::KeyQ,
        0x0013 => KeyCode::KeyR,
        0x001f => KeyCode::KeyS,
        0x0014 => KeyCode::KeyT,
        0x0016 => KeyCode::KeyU,
        0x002f => KeyCode::KeyV,
        0x0011 => KeyCode::KeyW,
        0x002d => KeyCode::KeyX,
        0x0015 => KeyCode::KeyY,
        0x002c => KeyCode::KeyZ,
        0x000c => KeyCode::Minus,
        0x0034 => KeyCode::Period,
        0x0028 => KeyCode::Quote,
        0x0027 => KeyCode::Semicolon,
        0x0035 => KeyCode::Slash,
        0x0038 => KeyCode::AltLeft,
        0xe038 => KeyCode::AltRight,
        0x003a => KeyCode::CapsLock,
        0xe05d => KeyCode::ContextMenu,
        0x001d => KeyCode::ControlLeft,
        0xe01d => KeyCode::ControlRight,
        0x001c => KeyCode::Enter,
        0xe05b => KeyCode::SuperLeft,
        0xe05c => KeyCode::SuperRight,
        0x002a => KeyCode::ShiftLeft,
        0x0036 => KeyCode::ShiftRight,
        0x0039 => KeyCode::Space,
        0x000f => KeyCode::Tab,
        0x0079 => KeyCode::Convert,
        0x0072 => KeyCode::Lang1, // for non-Korean layout
        0xe0f2 => KeyCode::Lang1, // for Korean layout
        0x0071 => KeyCode::Lang2, // for non-Korean layout
        0xe0f1 => KeyCode::Lang2, // for Korean layout
        0x0070 => KeyCode::KanaMode,
        0x007b => KeyCode::NonConvert,
        0xe053 => KeyCode::Delete,
        0xe04f => KeyCode::End,
        0xe047 => KeyCode::Home,
        0xe052 => KeyCode::Insert,
        0xe051 => KeyCode::PageDown,
        0xe049 => KeyCode::PageUp,
        0xe050 => KeyCode::ArrowDown,
        0xe04b => KeyCode::ArrowLeft,
        0xe04d => KeyCode::ArrowRight,
        0xe048 => KeyCode::ArrowUp,
        0xe045 => KeyCode::NumLock,
        0x0052 => KeyCode::Numpad0,
        0x004f => KeyCode::Numpad1,
        0x0050 => KeyCode::Numpad2,
        0x0051 => KeyCode::Numpad3,
        0x004b => KeyCode::Numpad4,
        0x004c => KeyCode::Numpad5,
        0x004d => KeyCode::Numpad6,
        0x0047 => KeyCode::Numpad7,
        0x0048 => KeyCode::Numpad8,
        0x0049 => KeyCode::Numpad9,
        0x004e => KeyCode::NumpadAdd,
        0x007e => KeyCode::NumpadComma,
        0x0053 => KeyCode::NumpadDecimal,
        0xe035 => KeyCode::NumpadDivide,
        0xe01c => KeyCode::NumpadEnter,
        0x0059 => KeyCode::NumpadEqual,
        0x0037 => KeyCode::NumpadMultiply,
        0x004a => KeyCode::NumpadSubtract,
        0x0001 => KeyCode::Escape,
        0x003b => KeyCode::F1,
        0x003c => KeyCode::F2,
        0x003d => KeyCode::F3,
        0x003e => KeyCode::F4,
        0x003f => KeyCode::F5,
        0x0040 => KeyCode::F6,
        0x0041 => KeyCode::F7,
        0x0042 => KeyCode::F8,
        0x0043 => KeyCode::F9,
        0x0044 => KeyCode::F10,
        0x0057 => KeyCode::F11,
        0x0058 => KeyCode::F12,
        0x0064 => KeyCode::F13,
        0x0065 => KeyCode::F14,
        0x0066 => KeyCode::F15,
        0x0067 => KeyCode::F16,
        0x0068 => KeyCode::F17,
        0x0069 => KeyCode::F18,
        0x006a => KeyCode::F19,
        0x006b => KeyCode::F20,
        0x006c => KeyCode::F21,
        0x006d => KeyCode::F22,
        0x006e => KeyCode::F23,
        0x0076 => KeyCode::F24,
        0xe037 => KeyCode::PrintScreen,
        0x0054 => KeyCode::PrintScreen, // Alt + PrintScreen
        0x0046 => KeyCode::ScrollLock,
        0x0045 => KeyCode::Pause,
        0xe046 => KeyCode::Pause, // Ctrl + Pause
        0xe06a => KeyCode::BrowserBack,
        0xe066 => KeyCode::BrowserFavorites,
        0xe069 => KeyCode::BrowserForward,
        0xe032 => KeyCode::BrowserHome,
        0xe067 => KeyCode::BrowserRefresh,
        0xe065 => KeyCode::BrowserSearch,
        0xe068 => KeyCode::BrowserStop,
        0xe06b => KeyCode::LaunchApp1,
        0xe021 => KeyCode::LaunchApp2,
        0xe06c => KeyCode::LaunchMail,
        0xe022 => KeyCode::MediaPlayPause,
        0xe06d => KeyCode::MediaSelect,
        0xe024 => KeyCode::MediaStop,
        0xe019 => KeyCode::MediaTrackNext,
        0xe010 => KeyCode::MediaTrackPrevious,
        0xe05e => KeyCode::Power,
        0xe02e => KeyCode::AudioVolumeDown,
        0xe020 => KeyCode::AudioVolumeMute,
        0xe030 => KeyCode::AudioVolumeUp,
        _ => return PhysicalKey::Unidentified(NativeKeyCode::Windows(scancode as u16)),
    })
}
