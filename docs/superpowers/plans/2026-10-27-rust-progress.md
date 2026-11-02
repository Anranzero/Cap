# Rust rewrite progress — plans: 2026-10-27-rust-rewrite.md

状态：用户已批准继续实施及五项依赖；原 C# 和桌面 EXE 保留。未创建 Git worktree：Cap 仓库无 HEAD 且有用户既有暂存/未暂存改动；用户指定 rust/ 隔离目录，不能通过提交用户改动来生成 worktree。独显管家当前没有独立 Git 仓库。此次不自动提交任何既有文件。

预检：Cap Task1 提供 Rect/Frame 给 Task2/Task3；Task1 只修改算法文件，Task2 只修改捕获/媒体/录制/恢复文件，Task3 协调者负责清单、lib/main、UI/设置/平台、构建。GPU 项目全独立，无共享文件。实现者必须在改变接口前通知协调者；不修改其他任务文件。

| Tasks | Interface/file overlap | Ruling |
| --- | --- | --- |
| Cap 1 ↔ 2 | Rect/Frame/GIF | 使用设计中的类型；补充签名写到本文件后通知 |
| Cap 1 ↔ 3 | 图像编辑/拼接/GIF | 所有算法暴露结果类型，UI 不复制算法 |
| Cap 2 ↔ 3 | 捕获、录制、恢复、预览 | 后端独立工作线程，UI 主线程消费结果 |
| GPU ↔ Cap | 无 | 两个独立 Cargo 工程，验证分别运行 |
| Cap 1/2/3/4 | 需求与验收一致 | 所有功能逐项对照，不以壳/占位完成 |

## Execution

- [x] Toolchain/linker/SDK native build verified (local CRT + SDK, static CRT, UTF-8 resources).
- [x] Cap algorithms implemented and independently reviewed; portable and native codec tests passed (ALGORITHMS.md). Public API alpha/timestamp boundaries documented in VERIFICATION.md.
- [x] Cap capture/media/recovery implemented and independently re-reviewed; 55 backend/library tests passed, 4 explicit helper/optional tests ignored in default suite (BACKEND.md).
- [x] Cap UI/platform integrated and independently reviewed; all reported issues fixed, scoped re-review passed.
- [x] GPU complete implementation independently reviewed; 16 tests, native UI/accessibility smoke and release checks passed (独显管家/rust/VERIFICATION.md).
- [x] Release/native verification: Cap full build gate, final published-exe smoke and PE checks passed; GPU release previously verified. Both final executables ready for delivery.

## UI review follow-up

- [x] Preserve all original WPF hotkey names during migration and capture.
- [x] Plain Enter inserts multiline text; Ctrl+Enter submits.
- [x] GIF playback accumulates deadlines instead of timer lateness.
- [x] Resized settings retains a reachable footer and scrollable content.
- [x] Restore tray icon after TaskbarCreated broadcast.
- [x] Run standalone Cap app smoke using only owned windows/profile/output.
- [x] Preserve Tab/ShiftTab navigation and cancel gestures after losing mouse capture.

All-target run after desktop unlock: 58 library + 46 binary + 23 portable test executions passed, 4 default ignores; OCR explicitly passed. Debug and final published Release app smoke passed all seven checks. Both PE import checks confirm no CLR/external CRT. Cap final SHA256 DC39B97C883A96CF9D7E8ACA459A823920C716812936AEBA86FD5F4909E8B3A7, GPU final SHA256 B8357A58B21938D06F01DA99ED10DA0FE631C32F460BAEB41510C420CFA3384A. Clipboard consumer testing correctly refuses WinSta0 fallback: private window-station creation is denied by this host (0x80070005); consumer checks remain unverified.

Original C# changes in Git predate this Rust effort; they remain untouched. Rust sources are isolated under rust/. No Git staging/commits or real user clipboard/startup writes are used for the smoke.
