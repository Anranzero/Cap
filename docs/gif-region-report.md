# 局部 GIF 录制范围修复

## 问题与批准范围

用户确认：框选了局部区域，录制时边框圈住整屏，但导出的 GIF 只包含正确选区。用户明确批准局部 GIF 使用现有 GDI 直接捕获，并显示真实选区细边框；全屏、窗口继续优先 WGC。

原因是区域 WGC 创建整显示器捕获对象，Windows 系统边框跟随显示器；每帧再裁剪到选区。因此保存尺寸正确，但可见范围提示错误。

## 实际修改

| 文件 | 改动 |
| --- | --- |
| `src/Cap.Recording/RecordingSession.cs` | 共用 `UsesDirectRegionCapture` 判断：仅 GIF、非指定窗口且不等于完整单个显示器时使用现有 GDI；其余路径保留 WGC 优先。 |
| `src/Cap.App/RecordingRegionIndicator.cs` | 四条窄 WPF 窗口标记真实物理选区，捕获排除、鼠标穿透、不激活；录制蓝色，暂停灰色。 |
| `src/Cap.App/RecordingControlWindow.cs` | 控制条持有选区边框，暂停/继续同步颜色，关闭时统一释放。 |
| `src/Cap.App/AppSmokeTest.cs` | 独立 EXE 冒烟检查局部 GIF 后端为 GDI，并实际解码校验选区尺寸。 |
| `src/Cap.App/Cap.App.csproj` | 修复版本为 1.2.1。 |
| `tests/Cap.Recording.Tests/Program.cs`、`SessionTests.cs` | 新增真实局部 GIF 后端、解码尺寸，以及负坐标、整屏、窗口、MP4 策略回归。 |
| `tests/Cap.Ui.Tests/Program.cs`、`RecordingRegionChecks.cs` | 原生边框坐标、扩展样式、捕获排除、命中穿透、暂停/继续与关闭清理检查。 |
| `README.md`、`docs/verification.md` | 说明局部 GIF 的后端例外及最终发布验收。 |

无新生产依赖，不请求系统无边框捕获权限，不改变 GIF 编码、鼠标坐标叠加或选区尺寸。

## 已实际验证

- 修复前新增真实录制检查返回 1，失败为：局部 GIF 后端实际是 `Windows Graphics Capture`，预期为直接 GDI 捕获。
- 修改后 `dotnet run --project tests/Cap.Recording.Tests -c Release` 返回 0，16,085 项断言通过，含 160×100 局部 GIF 真实解码、暂停/继续、恢复和目标关闭。
- UI 边框检查先复现缺少范围指示器的失败；实现后返回 0：真实物理位置与负原点、原生透明/不激活/工具窗样式、捕获排除、边缘/中心鼠标穿透、GDI 捕获前后逐像素相等、暂停/继续颜色及暂停时关闭清理通过。
- `./build.ps1` 返回 0，整体 Release 0 警告、0 错误，六个测试程序通过并发布 v1.2.1。
- 最新 UI 测试返回 0，包括既有功能条、热键、托盘、明暗主题及 MP4 真实播放定位。
- 最终 EXE 在独立目录、隔离 .NET 查找路径下冒烟返回 0：MP4 实际后端 WGC，局部 GIF 实际后端 GDI，640×360 GIF 尺寸校验及暂停/继续通过。结果位于本地 `.artifacts/portable-v121-final/smoke/smoke-result.json`。
- 本次只读代码复核未发现阻塞问题。旧版进程保持运行，旧 EXE 可恢复保留；新版运行需先退出旧版。

## 限制与尚未验证

GDI 捕获选区内当前可见桌面，遮挡窗口也会进入画面；指定窗口仍使用原有 WGC 路径。完整显示器 GIF 和 MP4 保留 Windows 捕获指示框。

单机原生验证不能替代实体多屏混合 DPI、HDR、远程桌面、热插拔与数小时高分辨率录制的硬件验收。没有故意制造系统调用部分失败，创建失败时逐条清理的路径经源码复核。没有打开浏览器或修改系统安全设置。
