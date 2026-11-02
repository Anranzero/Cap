# Settings compatibility and native layout verification

## Changes

- `src/settings.rs`: parses legacy WPF key names through one shared map, displays a parseable canonical name, and excludes left/right modifier keys consistently. Existing settings schema, validation rules, and persistence behavior are unchanged.
- `src/settings/hotkey_names.rs`: complete WPF-compatible names and aliases for numpad, lock, media, browser, OEM, ABNT, and IME keys; existing friendly names such as `Enter` and `Backspace` remain supported.
- `src/settings/wpf_hotkey_names.tsv`: independent fixture containing 190 non-modifier WPF enum names with their virtual-key codes. Generated once with Windows PowerShell 5.1, `PresentationCore`, `Enum.GetNames(Key)`, and `KeyInterop.VirtualKeyFromKey`. The Rust application has no WPF runtime dependency.
- `src/ui/settings.rs`: records control positions, responds to resizing/scrolling, filters all modifier virtual keys, and displays an explicit validation message when a captured key cannot be parsed. Posting failures are reported as UI errors. Bare Esc still cancels capture; Esc with a modifier is treated as a combination. Tab and Shift+Tab reach native focus navigation; Ctrl+Tab remains a capturable combination.
- `src/ui/settings_layout.rs`: initial placement stays inside the cursor monitor's work area, minimum tracking size is limited by that monitor, Save/Exit and tab navigation remain reachable, and the settings body scrolls independently. Key validation feedback stays above the pinned footer even in a short window. About text follows the scrollable body.
- `src/ui/settings_tests.rs`: native control/message tests against only windows created by the tests; they do not touch clipboard, registry, or actual user configuration.

No dependencies were added. Source files remain below 1000 lines. Before editing, the two existing source files were copied to `../Ai_Prompts/backups/settings-review-20261027/`; these copies were used only for diff review. UTF-8 decoding, original LF line endings, and Chinese strings were checked after editing.

## Reproduced failures

Before the compatibility change, the new parser and legacy-store tests failed with `无法识别快捷键：NUMPAD0` and `配置无效：无法识别快捷键：NUMPAD0`.

Before the native layout/capture change, all four initial native regressions failed: Save extended outside a 740×400 window; the bottom recording fields extended outside the client; left Shift (`VK_A0`) was incorrectly posted as a main key; and an unmappable captured key had no feedback control. A further short-window check reproduced hidden validation feedback before it was pinned above the footer. The added navigation regression also failed because plain Tab was swallowed before `IsDialogMessageW` could move focus.

## Actual verification

All Cargo commands used the toolchain loaded from `D:/Desktop/项目/.build-tools/Enter-RustBuild.ps1`.

- `cargo test --lib settings::tests -- --test-threads=1`: **8 passed**. Includes legacy config preservation, all 190 WPF aliases, all 15 modifier combinations for display/parse roundtrips, modifier-only rejection, validation failures, and existing atomic-save/backup behavior.
- `cargo test --bin cap-rust ui::settings::tests -- --test-threads=1`: **7 passed**. Covers short-window footer/minimum size, all five tabs, recording-field scrolling with preserved edits and Save events, extended-key capture and error feedback, modifier filtering, Tab/Shift+Tab focus navigation versus Ctrl+Tab capture, and work-area placement including negative monitor origins. Keyboard-state setup is scoped to the test thread and restored with a guard.
- `cargo clippy --lib -- -D warnings`: **passed**.
- `cargo clippy --bin cap-rust --tests -- -D warnings`: **passed**.
- Diff review against the two pre-edit copies: only the described changes; no unrelated Chinese text or line-ending changes.

The application-level full suite is coordinated by the parent worker and is not claimed here. These tests establish name/VK compatibility and native control accessibility; they do not physically press every IME/media key or prove that Windows will allow registration of every system-reserved combination. Registration failures continue through the existing explicit error path. No browser was opened.
