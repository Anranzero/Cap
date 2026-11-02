# Screenshot Toolbar Restoration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the original screenshot toolbar's recognizable vector icons and full hints instead of abbreviated Chinese characters.

**Architecture:** Keep the approved native Windows implementation, existing 19 tool command IDs, four mode commands, toolbar dimensions and placement. Render the original 24-unit icon semantics with Windows GDI+, and keep font/tooltip resources owned by each toolbar. This implements the existing approved requirement to preserve icons and Chinese operation semantics; it does not redesign capture behavior.

**Tech Stack:** Existing Rust + windows crate, native GDI/GDI+, common controls. No new production dependency.

**Spec:** `docs/superpowers/specs/2026-10-27-rust-rewrite-design.md`; visual references `src/Shared/VectorIcon.cs`, `src/Cap.Annotation/EditorToolbar.cs`, `CaptureModeBar.cs`.

## Global Constraints

- Only change the toolbar presentation and its private helpers/tests; keep mode and annotation command IDs, original C# and current capture fixes.
- Preserve 648/384 × 40 physical-pixel window geometry to avoid changing selector placement or clipping.
- No browser validation, no user clipboard/config/startup changes, no new dependency, no staging or commits.
- Back up changed UTF-8 sources and inspect actual diffs; each source remains below 1000 lines.

## Review Focus

- Native tooltip strings must stay alive while the tooltip window can reference them.
- Glyphs must be vectors independent of symbol-font availability; Chinese mode text must use a readable UI font.
- Click positions must retain the same commands, including finish/cancel and color/width menus.
- New hover painting must invalidate only on hover changes and preserve no-activation/remote visibility.
- GDI/GDI+ objects and tooltip windows must be released on creation failures and normal close.

### Task 1: Restore toolbar chrome and command visibility

**Files:** modify `rust/src/ui/toolbar.rs`; create `toolbar_icons.rs`, `toolbar_native.rs`, `toolbar_tests.rs` beside it.

- [x] Add a native tooltip/command-route regression; run it and observe the missing-tooltip failure.
- [x] Port the original icon shapes into a private GDI+ renderer; use icons for the 19 existing tool cells, distinct finish/cancel colors, separators, hover feedback and the current ink swatch.
- [x] Use an owned Microsoft YaHei UI font for mode labels; add native full Chinese tooltips with stable UTF-16 strings and no-activation behavior.
- [x] Keep click geometry and command IDs unchanged; test four modes and annotation command routes, tooltip counts/content and capture affinity.
- [x] Save native screenshot artifacts and inspect the toolbar visually; run the full binary suite, fmt, strict Clippy and release smoke.
- [x] Back up the current executable, gracefully restart only the verified Cap instance, deploy the tested artifact, and document actual results and remaining limits.

The existing screenshot paint buffer was reused for the toolbar by widening only its UI-internal visibility; selection paint behavior did not change. Result: 52 binary tests passed, strict Clippy/fmt/release passed, seven release-smoke checks passed. Visual artifacts were inspected at `rust/.artifacts/toolbar-visual-1793181300111331100/`. See `rust/VERIFICATION.md` for the published hash and remaining test limits.
