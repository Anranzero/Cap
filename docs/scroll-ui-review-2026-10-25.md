# 滚动截图交互修改与最终复核

## 1. 总体结论

已先完成交互修改，再进行静态复核和修正。本轮将原 530 × 163 的多行面板改为 408 × 48 的单行浮条，沿用点击“滚动截图”前的浮条位置，保留选区蓝色细边框。默认滚轮停稳后自动捕获；向下箭头启动自动滚动，暂停图标停止；勾号完成、叉号取消。

原因已从代码确认：旧流程没有传递模式栏位置，新的滚动窗口自行以选区右侧、上下侧和工作区边界重新定位；同时自动捕获默认为关闭，要求用户手动点“捕获下一屏”。

修改文件与职责：

| 文件 | 修改 |
| --- | --- |
| `src/Cap.Annotation/ScreenshotWorkflow.cs` | 结果附带可空的浮条物理坐标，不改变原 record 主构造。 |
| `src/Cap.Annotation/ScreenshotWindow.cs` | 选择滚动模式时计算浮条的屏幕绝对物理位置；未布局时保留空值。 |
| `src/Cap.App/App.xaml.cs` | 将位置传入滚动流程。 |
| `src/Cap.Annotation/ScrollingCaptureWorkflow.cs` | 简洁可拖动浮条、默认自动捕获、自动滚动状态、完成末屏、非模态显示、会话 Esc 取消、资源清理和边界定位。 |
| `src/Cap.Annotation/ScrollingCaptureInput.cs` | 向原目标窗口发送滚轮消息，不移动鼠标；检查遮挡、关闭及发送失败。 |
| `src/Cap.Annotation/ScrollingCaptureOutline.cs` | 四条 2 物理像素、鼠标穿透的选区外侧边框。 |
| `src/Cap.Annotation/ScrollingCaptureStitcher.cs` | 增加撤销后识别已保留正文视口的方法，复用原有比较规则。 |
| `tests/Cap.Scroll.Tests/NativeCaptureSmoke.cs` | 原有驱动改为等待自动捕获，并适配非模态异步完成；没有执行。 |
| `README.md` | 更新滚动截图操作说明。 |

没有增加生产依赖，没有改动截图批注、MP4/GIF 编码或用户设置。

## 2. 阻塞问题

未发现本轮静态检查范围内仍未处理的确定阻塞问题。原位置丢失、默认必须手动捕获和大面板问题已改。

## 3. 建议修改

最终复核中发现并已修复以下确定或有明确条件的风险：

- `FinishAsync`：末屏拼接失败不能立即关闭、静默输出缺少末屏的结果。现在保留纠正机会，并明确提示再次完成可以保存已截内容。
- `Append` / `FinishAsync`：长度达到上限时记录状态，直接允许完成已有结果，避免页面动画继续阻挡完成；不再重新启动越限捕获。
- `Undo`：清理过期暂停状态，等待返回保留内容后再拼接，避免刚撤销的内容立刻自动加回；正文比较避开拼接结果中只保留一次的固定页眉。
- `FinishAsync`：持续动画不能导致无法结束，允许再次完成已有结果；取消后异步等待不会再生成结果。
- `RunAsync` / `Loaded`：改为不抢焦点的非模态浮条，请求恢复目标窗口焦点，避免系统关闭“滚动非活动窗口”时必须额外点回页面。若 Windows 拒绝前台切换，显示简短的点击选区提示。
- 输入与关闭：滚动会话注册 Esc 取消并在关闭时注销；若注册被占用，仍可点叉号。Enter / Ctrl+Z 限浮条聚焦时，避免拦截用户在目标应用中输入。

这些修复通过控制流阅读和编译确认，不是已实测通过的结论。

## 4. 可选优化

未发现需要本轮扩展的可选优化。未改变拼接算法阈值或新增复杂配置。

## 5. 测试与验证

实际执行：

```powershell
dotnet build src/Cap.App/Cap.App.csproj -c Release --no-restore -warnaserror
dotnet build tests/Cap.Scroll.Tests/Cap.Scroll.Tests.csproj -c Release --no-restore -warnaserror
git diff --check
```

使用已有 cap-dev SDK。主程序和滚动工程编译成功，均 0 警告、0 错误。第二条只有编译，没有运行测试。核对了实际 diff、UTF-8 无 BOM 和 LF 换行。备份位于 `.artifacts/ui-backup/20261025-125126-scroll-toolbar`、`20261025-125111-scroll-position`、`20261025-125634-scroll-final-review`。

按用户要求未运行自动化或人工 UI 测试、未打开浏览器。现有开发预览监听器报告 `DEV_PREVIEW_READY iteration=25`，仅证明开发预览启动入口已到达。本轮未重新发布 `dist/cap.exe`。

## 6. 风险总结

尚未实际验证混合 DPI 跨屏拖动、不同目标应用的滚轮消息兼容性、Esc 冲突、持续动画、固定页眉的人工回滚及系统焦点策略。自动滚动不受目标程序支持时保留鼠标滚轮操作；自动滚动期间画面连续没有变化会停止继续发送滚轮。

开发预览继续允许捕获 cap 自身；浮条被拖到选区内时可能进入采样画面并影响拼接。选区边框画在选区外，不会占用输出像素。普通运行仍使用已有捕获排除策略。
