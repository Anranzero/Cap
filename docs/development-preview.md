# 开发预览与桌面入口

## 两个明确入口

- **cap 开发预览**：运行 `dev.ps1`，使用已有 .NET 8 SDK 的 `dotnet watch --no-hot-reload` 监视源码。保存后自动编译并重开真实 WPF 窗口。
- **cap 测试版**：运行 `dist/cap.exe --settings`，设置标题显示打包版本。这是手动发布后的独立 EXE，源码修改不会直接影响它。

旧的空白 `cap.lnk` 已保留到 `.artifacts/ui-backup/20261025-tray-shortcut/desktop-cap-old.lnk`。两个新入口均直接引用 EXE 内置图标，不再引用额外 ICO 路径；已请求 Windows Shell 更新图标缓存，没有删除系统缓存文件或重启 Explorer。

## 实现范围

- `dev.ps1`：定位已有 SDK、单个监视器互斥、重复点击时刷新现有预览、禁用浏览器启动；不修改发布目录。
- `tests/Cap.Ui.Tests/DeveloperPreview.cs`：复用真实 App、设置、截图和录制实现；独立设置及输出，不调用系统自启动；窗口标题显示预览编号，拥有明确的托盘标识和退出路径。
- `tests/Cap.Ui.Tests/Cap.Ui.Tests.csproj`：显式监视 C#、XAML、项目、图标及重开触发文件。
- `tests/Cap.Ui.Tests/Program.cs`：`--preview` 进入开发预览，原有自动化测试模式保留。
- `src/Cap.App/App.xaml.cs`：托盘增加退出；重复启动不再静默退出，正常启动不再发送“已就绪”通知。
- `src/Cap.App/SettingsWindow.cs`：显示当前运行版本。

## 实际验证

- Debug 预览项目构建 0 警告、0 错误。
- `dotnet watch --list` 实际包含 `EditorToolbar.cs`、`Themes/Controls.xaml` 和重开触发文件。
- 实际监视日志先记录 `DEV_PREVIEW_READY iteration=1 pid=30716`；修改真实 C# 窗口标题后自动启动 `iteration=2 pid=25908`，证明无需重新打包即可更新代码。
- 再次执行 `dev.ps1` 返回 0，只请求已有监视器重开，没有创建第二个监视器。
- Windows Shell 直接读取两个桌面快捷方式的 48/64/96px 图像均为所选相机图案。最初用户仍反馈空白，因此未仅据 API 提取结果判定成功；将两者改回 EXE 内置图标并刷新 Shell 图标缓存后，用户于 2026-10-25 明确确认“已显示蓝色相机”，且截图来源为 Windows 真正的桌面。桌面显示问题现已由用户实际确认解决。

## 使用边界

这是自动重启式预览，运行时状态不会跨重启保留；先完成录制或批注，再保存源码。编译失败时保留错误日志，修复后保存会再次尝试。预览快捷键使用 Ctrl+Shift+F10/F11/F12，与打包版默认快捷键分开。

所有预览设置与输出均在项目 `.artifacts/dev-preview`，不覆盖正式设置，不开启系统自启动，不需要浏览器。

开发预览允许截图自己的界面：触发截图时保留设置窗口，截图工具栏、贴图和录制控制条也不设置捕获排除，方便保存问题截图。此开关仅由开发预览入口开启，正式版仍隐藏设置窗口并排除工具浮层。开发预览录制时若控制条位于选区内，也会被录入，可将它拖到选区外。
