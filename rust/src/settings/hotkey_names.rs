//! WPF Key names retained for settings written by the original HotkeyInput.
//! Aliases/VKs were checked with PresentationCore KeyInterop; the first name is
//! the stable display name. Modifier keys deliberately have no entry.
pub(super) const KEY_NAMES: &[(u32, &[&str])] = &[
    (0x03, &["Cancel"]),
    (0x08, &["Backspace", "Back"]),
    (0x09, &["Tab"]),
    (0x0c, &["Clear"]),
    (0x0d, &["Enter", "Return"]),
    (0x13, &["Pause"]),
    (0x14, &["CapsLock", "Capital"]),
    (0x15, &["KanaMode", "HangulMode"]),
    (0x17, &["JunjaMode"]),
    (0x18, &["FinalMode"]),
    (0x19, &["HanjaMode", "KanjiMode"]),
    (0x1b, &["Escape", "Esc"]),
    (0x1c, &["ImeConvert"]),
    (0x1d, &["ImeNonConvert"]),
    (0x1e, &["ImeAccept"]),
    (0x1f, &["ImeModeChange"]),
    (0x20, &["Space"]),
    (0x21, &["PageUp", "Prior"]),
    (0x22, &["PageDown", "Next"]),
    (0x23, &["End"]),
    (0x24, &["Home"]),
    (0x25, &["Left"]),
    (0x26, &["Up"]),
    (0x27, &["Right"]),
    (0x28, &["Down"]),
    (0x29, &["Select"]),
    (0x2a, &["Print"]),
    (0x2b, &["Execute"]),
    (0x2c, &["PrintScreen", "Snapshot"]),
    (0x2d, &["Insert"]),
    (0x2e, &["Delete"]),
    (0x2f, &["Help"]),
    (0x5d, &["Apps"]),
    (0x5f, &["Sleep"]),
    (0x60, &["NumPad0"]),
    (0x61, &["NumPad1"]),
    (0x62, &["NumPad2"]),
    (0x63, &["NumPad3"]),
    (0x64, &["NumPad4"]),
    (0x65, &["NumPad5"]),
    (0x66, &["NumPad6"]),
    (0x67, &["NumPad7"]),
    (0x68, &["NumPad8"]),
    (0x69, &["NumPad9"]),
    (0x6a, &["Multiply"]),
    (0x6b, &["Add"]),
    (0x6c, &["Separator"]),
    (0x6d, &["Subtract"]),
    (0x6e, &["Decimal"]),
    (0x6f, &["Divide"]),
    (0x90, &["NumLock"]),
    (0x91, &["Scroll"]),
    (0xa6, &["BrowserBack"]),
    (0xa7, &["BrowserForward"]),
    (0xa8, &["BrowserRefresh"]),
    (0xa9, &["BrowserStop"]),
    (0xaa, &["BrowserSearch"]),
    (0xab, &["BrowserFavorites"]),
    (0xac, &["BrowserHome"]),
    (0xad, &["VolumeMute"]),
    (0xae, &["VolumeDown"]),
    (0xaf, &["VolumeUp"]),
    (0xb0, &["MediaNextTrack"]),
    (0xb1, &["MediaPreviousTrack"]),
    (0xb2, &["MediaStop"]),
    (0xb3, &["MediaPlayPause"]),
    (0xb4, &["LaunchMail"]),
    (0xb5, &["SelectMedia"]),
    (0xb6, &["LaunchApplication1"]),
    (0xb7, &["LaunchApplication2"]),
    (0xba, &["OemSemicolon", "Oem1"]),
    (0xbb, &["OemPlus"]),
    (0xbc, &["OemComma"]),
    (0xbd, &["OemMinus"]),
    (0xbe, &["OemPeriod"]),
    (0xbf, &["OemQuestion", "Oem2"]),
    (0xc0, &["OemTilde", "Oem3"]),
    (0xc1, &["AbntC1"]),
    (0xc2, &["AbntC2"]),
    (0xdb, &["OemOpenBrackets", "Oem4"]),
    (0xdc, &["OemPipe", "Oem5"]),
    (0xdd, &["OemCloseBrackets", "Oem6"]),
    (0xde, &["OemQuotes", "Oem7"]),
    (0xdf, &["Oem8"]),
    (0xe2, &["OemBackslash", "Oem102"]),
    (0xe5, &["ImeProcessed"]),
    (0xf0, &["OemAttn", "DbeAlphanumeric"]),
    (0xf1, &["OemFinish", "DbeKatakana"]),
    (0xf2, &["OemCopy", "DbeHiragana"]),
    (0xf3, &["OemAuto", "DbeSbcsChar"]),
    (0xf4, &["OemEnlw", "DbeDbcsChar"]),
    (0xf5, &["OemBackTab", "DbeRoman"]),
    (0xf6, &["Attn", "DbeNoRoman"]),
    (0xf7, &["CrSel", "DbeEnterWordRegisterMode"]),
    (0xf8, &["ExSel", "DbeEnterImeConfigureMode"]),
    (0xf9, &["EraseEof", "DbeFlushString"]),
    (0xfa, &["Play", "DbeCodeInput"]),
    (0xfb, &["Zoom", "DbeNoCodeInput"]),
    (0xfc, &["NoName", "DbeDetermineString"]),
    (0xfd, &["Pa1", "DbeEnterDialogConversionMode"]),
    (0xfe, &["OemClear"]),
];

pub(super) fn key_code(name: &str) -> Option<u32> {
    let name = name
        .strip_prefix('D')
        .filter(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit())
        .unwrap_or(name);
    if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
        return Some(name.as_bytes()[0].to_ascii_uppercase() as u32);
    }
    if let Some(f) = name
        .strip_prefix('F')
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|f| (1..=24).contains(f))
    {
        return Some(0x6f + f);
    }
    KEY_NAMES.iter().find_map(|(key, names)| {
        names
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
            .then_some(*key)
    })
}

pub(super) fn key_name(key: u32) -> String {
    match key {
        0x30..=0x39 | 0x41..=0x5a => (key as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", key - 0x6f),
        _ => KEY_NAMES
            .iter()
            .find(|(code, _)| *code == key)
            .map(|(_, names)| names[0].to_string())
            .unwrap_or_else(|| format!("Key{key}")),
    }
}
