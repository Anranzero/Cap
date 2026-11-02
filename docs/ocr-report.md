# 离线文字提取实现与验证

> 2026-10-25 交互更新：选区菜单直接识别并复制文字，成功时只显示简短提示，不再打开 `TextExtractWindow`。空结果不修改剪贴板，剪贴板占用会有限重试，持续失败明确提示。以下是初版实现与当时的验证记录；其中结果窗口说明属于旧流程，不表示本次重新运行了这些测试。

## 2026-10-25 本次修改与最终复核

1. **总体结论**：先修改、后静态复核。`App.xaml.cs` 的提取文字分支改为直接复制，成功只给托盘简短提示；`README.md` 更新操作说明，本文件区分本次结果和历史验证。只修改这三个文件，无新增依赖或测试。
2. **阻塞问题**：未发现本次引入的确定问题。`Document.Export(Selection)` 以选区宽高渲染裁剪图，OCR 仅处理该图，没有重新抓取全桌面。普通流程不再创建结果窗口。
3. **建议修改**：未发现未处理的确定建议项。已覆盖空白结果不写剪贴板、保留多行换行、临时占用最多重试 4 次（间隔 80 ms）、持续失败明确提示未复制，以及 OCR 返回后和重试间退出程序时停止写入。未修改 OCR 算法和语言模型选择。
4. **可选优化**：未发现需要扩大本次范围的优化，旧结果窗口类保留但不再由提取入口打开。
5. **测试与验证**：实际执行 `dotnet build src/Cap.App/Cap.App.csproj -c Release --no-restore -warnaserror`，成功，0 警告、0 错误；`git diff --check` 返回 0。已对照 `.artifacts/ui-backup/20261025-130502-direct-ocr` 检查实际差异，保持 UTF-8 无 BOM、LF。遵照用户要求未运行测试或 UI 验证，未打开浏览器。开发监听器报告 `DEV_PREVIEW_READY iteration=28`，只代表预览启动入口已到达。
6. **风险总结**：真实 OCR 准确度、系统剪贴板竞争、系统是否抑制通知尚未经本次运行验证。编译成功不代表以上场景已通过。本轮未重新打包 `dist/cap.exe`。

## 初版实现与历史验证记录

新增文件：

- `src/Cap.Annotation/OcrService.cs`：公共入口 `Task<string> OcrService.ExtractTextAsync(BitmapSource image, CancellationToken token = default)`。
- `src/Cap.App/TextExtractWindow.cs`：内部 WPF 结果窗口，构造参数为识别文本，提供多行只读选择、复制全部、字符数和关闭操作。
- `tests/Cap.Ocr.Tests/`：无第三方测试依赖的可执行离线测试项目，仅引用 Annotation 项目。

## 实际行为

服务使用 Windows 内置 `Windows.Media.Ocr.OcrEngine`，优先创建系统用户语言识别器；失败时依次尝试本机已安装的中文、英文及其他识别语言。没有可用模型时明确提示用户安装 Windows OCR 语言包，不伪造识别结果，不联网、不自动下载模型。

输入仅为调用者提供的截图或裁剪后的 `BitmapSource`。超过 `OcrEngine.MaxImageDimension` 时等比例缩小；转换为 BGRA 后将透明像素合成到白底，并通过 `DataWriter.DetachBuffer` 和 `SoftwareBitmap.CreateCopyFromBuffer` 传入 Windows OCR。识别结果保留引擎返回的每行内容及换行。没有引入生产依赖。

取消请求在识别前和原生识别完成后检查。原生操作进行中不提前释放 `SoftwareBitmap`，避免取消导致 Windows 读取已释放缓冲区；因此识别已经开始时，取消完成可能等待当前原生识别结束。

结果窗口复用 `Ui.PrepareWindow`、`Ui.WindowHeader`、现有卡片和主题资源。只在用户点击“复制全部”时写剪贴板；打开窗口不会自动覆盖剪贴板，空结果显示重新框选的提示。

## 已实际运行

SDK 路径采用本机已有的 `%LOCALAPPDATA%/cap-dev/dotnet/dotnet.exe`，以下命令均在项目根目录执行。

```powershell
dotnet run --project tests/Cap.Ocr.Tests/Cap.Ocr.Tests.csproj
dotnet run --project tests/Cap.Annotation.Tests/Cap.Annotation.Tests.csproj
dotnet build src/Cap.App/Cap.App.csproj --no-restore
```

- 先运行新增测试，确认因尚不存在 `OcrService` 入口而失败，再实现服务。
- OCR 测试最终通过；本机 Windows OCR 模型清单为 `zh-Hans-CN`。
- 白底黑字大字号图片的真实识别结果为 `CAP SCREENSHOT 12345`。
- 已通过：null 输入、开始前取消、上采样来源、多行换行、1×1 白色图片、全透明隐藏 RGB 文字、超出引擎尺寸上限的来源自动缩小，以及裁剪排除区域外文本。
- 现有 Annotation 测试通过，涵盖选区、马赛克/模糊、裁剪分辨率、标注历史、每种标注工具和原子导出。
- App 编译通过，0 警告、0 错误。
- 已检查两个新增源码文件的完整新增 diff，没有修改旧文件、接口契约或其他模块。

## 尚未验证与限制

- 当前机器有 OCR 模型，未实际卸载系统模型；无模型错误分支依代码阅读判断。测试在无模型环境会验证明确错误并报告识别测试 `SKIP`，不会把跳过报告为识别通过。
- 不保证任意字体、低清晰度、旋转文字或任意语言的识别准确率；识别语言范围由 Windows 已安装模型决定。
- 当前子任务只编译结果窗口；明暗主题渲染、按钮点击、剪贴板交互及截图模式到此入口的完整路由由主任务集成验证。
- 未打开浏览器。
