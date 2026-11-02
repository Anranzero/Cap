# 选区直接录制：修改与复核

## 1. 总体结论

用户选择选区后点击“录屏”，现在直接启动录制，不再经过录制设置窗口。先完成代码修改，再复核调用链与实际 diff。默认 MP4；通过 GIF 快捷键进入时使用 GIF。所有入口的选区菜单采用同一行为，截图批注栏的录屏按钮也复用此路径。

修改文件：

- `src/Cap.App/App.xaml.cs`：`ExecuteAsync` 调用 `StartRegionRecording`，完整读取已保存的录制参数；共用输出路径生成；高级配置改为单独的 `ConfigureRecordingAsync`，仅由设置页主动进入；设置窗口恢复标志在启动前保存。
- `src/Cap.App/SettingsWindow.cs`：更新录制页说明，提供主动进入窗口/全屏录制的次要入口，保留高级目标和格式选择能力。
- `tests/Cap.Ui.Tests/DeveloperPreview.cs`：为该设置入口接线，开发预览与主程序行为一致。
- `README.md`：更新直接录制及高级配置的操作说明。

没有新增依赖、配置字段或测试，没有修改捕获、编码、暂停、停止和预览实现。

## 2. 阻塞问题

未发现本次改动引入的确定阻塞问题。原先所有录制都必经 `RecordingSetupWindow.ShowDialog` 的额外步骤已从选区菜单路径移除。

## 3. 建议修改

未发现尚需处理的确定建议项。已检查：

- MP4 使用保存的帧率、原始尺寸；GIF 使用保存的帧率和最大宽度。
- 选区边界保持绝对物理坐标，区域录制不误带窗口句柄。
- 指针、点击高亮、坐标开关、相对/绝对坐标、字号及颜色全部从已保存设置映射。
- 文件目录、MP4/GIF 扩展名和原有时间戳命名一致。
- `busy` / `session` 门禁保留；高级参数取消、区域取消不启动录制。
- 高级入口的指定窗口和显示器选项不被区域默认值覆盖。
- 保存目录或启动失败时仍走错误提示及设置窗口恢复路径。

以上是静态控制流判断，不代表已执行交互验证。

## 4. 可选优化

未发现本次需求必须加入的可选优化，未扩展重构。

## 5. 测试与验证

实际执行 `dotnet build src/Cap.App/Cap.App.csproj -c Release --no-restore -warnaserror`，使用既有 cap-dev SDK：成功，0 警告、0 错误。

`git diff --check` 返回 0。核对修改前备份与实际补丁，4 个修改文件保持 UTF-8 无 BOM、LF。备份位于 `.artifacts/ui-backup/20261025-130101-direct-record` 和 `.artifacts/ui-backup/20261025-130146-direct-record-settings`。

按用户要求未运行测试、未操作 UI、未打开浏览器。现有开发监听器报告 `DEV_PREVIEW_READY iteration=27`，表明预览启动入口已到达；没有据此宣称实际录制通过。

## 6. 风险总结

真实首帧捕获、混合 DPI 录制、窗口焦点及错误时序尚未经运行验证。本轮未发布 `dist/cap.exe`；改动可从现有桌面“cap 开发预览”查看。
