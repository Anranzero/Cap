# 设置、开发预览与截图交互复核（2026-10-25）

## 1. 总体结论

按用户要求先修改功能，最后静态复核。本轮未运行自动化测试或人工 UI 测试，未打开浏览器。检查范围为设置输入、主题保存、开发预览接线、截图模式栏、选区状态、结果窗口激活、配置备份及录制恢复日志释放。确认的问题均已在对应调用链内修复，未引入生产依赖。

这份记录对应当前工作区，不替代旧的发布验收报告；没有声称已确认全部运行时行为。

## 2. 阻塞问题

未发现本轮检查范围内仍未处理的确定阻塞问题。以下为已修复的确定问题，结论来自代码控制流和实际补丁检查。

| 位置 | 原问题与影响 | 已采取的修改 |
| --- | --- | --- |
| `src/Cap.Annotation/CaptureModeBar.cs`、`ScreenshotWindow.cs` | 模式栏点阵只是图形，没有拖动事件；即使仅改变位置，`Update` 也会重新定位。 | 左侧拖动柄接入鼠标捕获；用桌面相对物理坐标保存位置，刷新时保留，按鼠标所在显示器限制边界。重新框选时恢复自动定位。 |
| `ScreenshotWindow.PointerDown` | 已有选区时，Hover 不随鼠标更新，再点击其他窗口可能吸附旧窗口。 | 按下时重新计算窗口命中，再记录本次吸附目标。 |
| `ScreenshotWindow` 右键处理 | 左键批注过程中右键清空选区，没有结束拖动；后续移动可能对空选区执行 Clamp 并抛异常。 | 先结束拖动并释放鼠标捕获，再清理预览及选区。 |
| `src/Cap.System/SettingsStore.cs` | 读取损坏配置后保存默认值，会把损坏文件写入 `.bak`，覆盖正常备份。 | 保存前验证旧文件；正常文件更新 `.bak`，损坏文件保留到唯一的 `settings.corrupt.*.json`。读取或备份失败时不覆盖配置；清理临时文件的失败记录日志。 |
| `src/Cap.Recording/RecordingRecovery.cs`，`FrameJournal` | 构造中写入/Flush 失败会遗留文件句柄；Dispose 中 writer 抛异常会跳过 stream 释放。 | 构造失败时释放流；Dispose 各清理步骤分别执行，单异常保留类型与堆栈，多异常聚合。 |

## 3. 建议修改

未发现本轮范围内尚未处理的确定建议项。以下已修复：

| 位置 | 原问题与影响 | 已采取的修改 |
| --- | --- | --- |
| `src/Cap.App/App.xaml.cs`，`ExecuteAsync` / `StopRecordingCoreAsync` | 结果窗口打开后再次激活设置，OCR、长截图或录制预览被设置页盖住。 | 恢复设置后优先激活本次仍可见的结果窗口；取消或失败仍恢复设置。 |
| `App.SaveSettings`、`tests/Cap.Ui.Tests/DeveloperPreview.cs` 的保存回调 | 保存失败时忽略旧热键恢复失败，界面提示不足以解释实际快捷键状态；自启动回滚失败也只留日志。 | 检查恢复返回值和异常，合并原始保存错误与回滚错误，提示并记录完整异常。 |
| `DeveloperPreview.cs` | “恢复中断的录制”没有订阅事件，开发版点击无操作。 | 连接到已有的 `App.RecoverAsync`。 |

本轮同时保留并复核先前要求的设置修改：

- `HotkeyInput.cs`、`SettingsWindow.cs`、`HotkeyService.cs`：按真实组合键录入；同一设置页的重复快捷键即时提示，保留原有效值；录入已注册快捷键时优先交给输入框，避免触发截图。
- `SettingsWindow.cs`、`App.xaml.cs`、`DeveloperPreview.cs`：主题选择即时应用并单独保存；保存失败回退；底部“退出程序”左对齐，取消和保存靠右。
- `ColorPickerButton.cs`、`SettingsWindow.cs`、`RecordingSetupWindow.cs`：预设色与二级自定义 RGB 面板，未确认的自定义颜色不写入设置。
- `RecordingControlWindow.cs`、`App.xaml.cs`：停止后立即显示保存状态并禁用重复操作。
- `CaptureService.cs`、`App.xaml.cs`、`DeveloperPreview.cs`：开发预览允许捕获自身窗口，也不在截图前隐藏设置；普通启动仍沿用原捕获排除策略。
- `README.md`、`docs/development-preview.md`：相关操作与开发预览行为说明。

## 4. 可选优化

未发现需要在本轮额外修改的可选优化。未为了风格一致扩大重构。

## 5. 测试与验证

实际执行的验证：

```powershell
dotnet build src/Cap.App/Cap.App.csproj -c Release --no-restore -warnaserror
dotnet build tests/Cap.Ui.Tests/Cap.Ui.Tests.csproj -c Release --no-restore -warnaserror
git diff --check
```

使用项目已有的 cap-dev .NET SDK。两个构建均成功，0 警告、0 错误；第二条仅编译开发预览所在工程，没有执行测试。检查了本轮源码与修改前备份的实际 diff，未发现无关格式变更。

按用户要求未新增或运行测试，未做 UI 操作验证。已有开发预览监听器自动重新编译并报告 `DEV_PREVIEW_READY iteration=19`；这只能证明预览启动入口已到达，不能作为功能测试通过的证据。本轮没有发布或替换 `dist/cap.exe`。

尚需运行验证的场景（本轮未执行）：跨不同 DPI 显示器拖动及边缘手感、窗口失焦时的鼠标捕获、系统占用快捷键、写入失败和回滚失败、录制停止后的前台窗口顺序、真实磁盘耗尽时的日志释放。

## 6. 风险总结

源码层面已修复确认的交互和异常路径问题。编译通过不等于实际功能通过，尤其不能据此保证混合 DPI、系统焦点和故障注入场景。开发版允许自身窗口进入录制画面，录制控制条若放在选区内也会被录入；这是本次开发预览的明确行为。

本次拖动手动位置仅保留在当前截图会话内，重新框选时回到选区附近，不写入全局设置。未修改图标、桌面快捷方式或已发布程序。
