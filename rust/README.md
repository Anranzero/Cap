# Cap Rust

原生 Windows 截图、批注、滚动截图、离线 OCR、MP4/GIF 录制与预览程序。实现使用 Rust、Windows API 和系统编解码器；独立运行，无需 .NET、WebView 或 FFmpeg。原 C# 工程和旧版 dist/cap.exe 保留。

## 运行与配置

发布文件：`dist/cap-rust.exe`。双击后驻留托盘，使用托盘菜单或 `--settings` 打开设置。正式配置保存在 `%LOCALAPPDATA%\cap-rust`；首次启动读取兼容的 `%LOCALAPPDATA%\cap\settings.json`，不覆盖旧配置。默认快捷键和旧版共享，单实例锁也与旧版一致，应先退出旧版再运行。

`cap-rust.exe --profile <目录> --settings` 使用独立开发配置，保存设置时不会写入系统自启动。`--startup` 用于登录启动。

截图后可调整选区，选择批注、贴图、滚动截图、文字提取或录制。文字支持多行；录制支持区域、窗口、显示器及整个桌面，暂停/继续与保存后预览。无音频录制；所有处理在本机完成。

## 构建

在此目录运行 `./build.ps1`，执行格式检查、测试、严格 Clippy 和 Release 构建。若存在项目共用 `.build-tools/Enter-RustBuild.ps1`，脚本自动加载本机便携 SDK/CRT 链接环境；也可以使用常规 Rust MSVC + Windows SDK 环境。

依赖版本由 Cargo.lock 锁定。`--smoke-test <空目录>` 执行显式端到端诊断，使用独立配置、自有窗口和指定输出，生成 JSON 与截图/录制样本。诊断不会注册正式快捷键或写入用户自启动。

## 验证记录

专项实现与验证见 ALGORITHMS.md、BACKEND.md、PREVIEW.md、SCROLLING.md、RECORD_UI.md 及最终 VERIFICATION.md。混合 DPI、多种显卡驱动与长时间录制需要在实际环境继续验收；Windows 捕获/编解码服务和驱动调用本身没有硬超时保证。
