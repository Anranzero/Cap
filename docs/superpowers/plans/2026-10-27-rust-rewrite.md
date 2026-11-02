# Cap Rust Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 rust/ 交付具备现有完整功能的独立原生 Rust Cap。

**Architecture:** 纯像素算法与 Win32/WinRT 后端分离；所有 UI 由原生 Windows 窗口承载。原 C# 文件作为行为参考，保持不变。

**Tech Stack:** Rust 2024，windows 0.61.3，serde/serde_json，flate2；Windows GDI/WGC/D3D11/MF/WIC/OCR。

**Spec:** `docs/superpowers/specs/2026-10-27-rust-rewrite-design.md`

## Global Constraints

- 新工程只放在本项目 `rust/`，不改原 C# 业务逻辑，不提交用户既有暂存改动。
- Windows 10 2004+/11 x64；物理像素/负坐标；不增加录音或网络功能。
- 兼容 PascalCase settings.json 和 CAPJ0001 日志。
- 不打开浏览器验收，不修改用户 GPU 偏好和自启动。
- 源文件原则上低于 1000 行；输出 rust/dist/cap-rust.exe，不替换桌面入口。

## Review Focus

1. 负坐标或跨屏选区必须保持精确像素；geometry 边界测试负责。
2. 目标关闭/重复停止只发布完整媒体，异常保留日志；recording 集成测试负责。
3. 恢复文件残缺、恶意长度和不同格式必须拒绝或安全截断；recovery 测试负责。
4. 滚动中的固定栏/重复背景必须拒绝歧义，不悄悄拼错；scroll 像素测试负责。
5. 剪贴板被占用、热键冲突、配置损坏必须明确提示并保留原数据；platform 集成测试负责。

### Task 1: 像素核心、批注、滚动和 GIF

**Files:** `rust/src/{geometry,pixels,annotation,scroll,gif}.rs`，可按职责增加同名模块子文件；测试在对应模块或 rust/tests。
**Interfaces:** `Rect::new(i32,i32,u32,u32)->Result<Rect,String>`；`Frame::new(u32,u32,Vec<u8>)->Result<Frame,String>`；BGRA32 顶向下。模块公开接口先在 progress.md 写出并通知协调者。
- [ ] 测试先覆盖裁剪/负坐标、批注像素/撤销、滚动歧义和 GIF 原生解码/延时；记录实际 RED。
- [ ] 实现现有算法，保持有界内存；通过测试并记录 GREEN。
- [ ] 交由独立评审检查与原实现的功能对照及边界。

### Task 2: Windows 捕获、媒体与恢复

**Files:** `rust/src/{capture,recording,recovery,media,overlays}.rs` 及模块子文件。
**Consumes:** geometry::Rect 和 pixels::Frame。
**Produces:** 显示器/窗口枚举、瞬时 GDI 截图、录制会话 Start/Pause/Resume/Stop、MP4/GIF 写入、恢复枚举与重编码；具体 Rust 签名在实现前写入 progress.md。
- [ ] 先用合成帧和旧日志固定帧/时间/损坏边界测试，执行 RED。
- [ ] 实现捕获和编码；目标移动/关闭、CPU 回读、局部 GIF/GDI 和 WGC 回退。
- [ ] 验证真实截图、MP4/GIF 解码、暂停和恢复；报告硬件未覆盖项。

### Task 3: 原生界面与系统集成

**Files:** `rust/Cargo.toml`、`rust/build.rs`、`rust/resources/`、`rust/src/{lib,main,app,settings,platform,ui}.rs` 和子文件、`rust/build.ps1`。
**Consumes:** Tasks 1/2 的公开接口。
- [ ] 设置、快捷键和选择模式/状态转换先写行为测试，执行 RED。
- [ ] 实现托盘、冻结选区/双工具栏、批注、滚动/OCR、录制配置/控制、贴图和媒体预览。
- [ ] 保持配置/备份格式，隔离开发设置，不自动启用自启动；不调用旧 EXE。
- [ ] cargo test、fmt、clippy、Release 构建与原生 smoke 后进行独立评审。

### Task 4: 集成与交付

**Files:** `rust/README.md`、`rust/VERIFICATION.md`、`rust/dist/cap-rust.exe`。
- [ ] 对照全部现有功能，修复评审中的确定问题。
- [ ] 测量发布包体与本机内存；记录真实命令/结果及未覆盖场景。
- [ ] 只交付通过验证的 Rust 产物，明确剩余限制。
