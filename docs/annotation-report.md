# Cap.Annotation 实现与验证

## 接入 API

命名空间 `Cap.Annotation`，运行在已启动的 WPF `Application` 中；建议在主 UI 线程调用。后台线程调用会调度到 WPF Dispatcher。截图前应由调用方隐藏主窗口并等待其消失。

```csharp
ScreenshotResult? screenshot = await ScreenshotWorkflow.RunAsync(defaultSaveDirectory, darkTheme);
PixelRect? recordingRegion = await ScreenshotWorkflow.SelectRegionAsync(darkTheme);
```

`ScreenshotResult` 字段：

- `Bounds`：桌面绝对物理像素坐标，支持负 X/Y。
- `Image`：冻结的 `BitmapSource`，尺寸等于最终选区尺寸，96 DPI，便于直接保存或跨线程读取。
- `SavedPath`：保存成功后的文件名；复制、贴图时为空。
- `Copied`、`Pinned`：对应完成方式。

Esc / 关闭返回 `null`。区域选择模式只返回绝对 `PixelRect?`，不会写剪贴板、保存图片或创建贴图。

## 文件与职责

- `ScreenshotWorkflow.cs`：捕获冻结桌面、读取捕获前窗口列表、UI 调度和结果契约。
- `ScreenshotWindow.cs`：截图覆盖窗、拖动与键盘交互、工具切换、剪贴板、保存、贴图。
- `ScreenshotExport.cs`：先编码并刷新同目录唯一临时文件，成功后原子替换目标，失败保留原文件并清理本次临时文件。
- `EditorSurface.cs`：冻结桌面、选区外遮罩、选择框、尺寸和八个手柄、绘制预览。
- `SelectionGeometry.cs`：选区规范化、越界裁剪、移动、八向缩放和手柄命中。
- `EditorToolbar.cs`：圆角浮动工具栏，所有工具的实际入口、颜色、粗细、撤销、重做和完成操作。
- `AnnotationDocument.cs`：以物理像素定义的绘制命令、精确尺寸导出、历史与栅格效果。
- `NativeDesktop.cs`：物理桌面鼠标、窗口定位、鼠标所在显示器查询。
- `TextPrompt.cs`：多行文字输入（Ctrl+Enter 添加），定位到当前显示器。
- `PinWindow.cs`：置顶贴图，移动、缩放、复位、复制和关闭。
- `AssemblyInfo.cs`：仅向测试程序集开放内部实现。
- `tests/Cap.Annotation.Tests/`：无额外 NuGet 依赖的 STA 控制台验证程序。

无新增生产依赖。未改动 Core、Capture、App 或公共构建配置。

## 实际行为

1. 捕获虚拟桌面后创建覆盖窗。窗口列表也在覆盖窗出现前读取，避免覆盖窗本身截获窗口吸附。
2. 悬停窗口显示其边界，单击选定窗口；拖动建立任意矩形选区。选区可以拖动、八向缩放，也可用方向键移动（Shift 为 10 像素）。
3. 工具栏在框选、移动、缩放和绘制时隐藏；完成后放在选区下方或上方，并限制在选区右下角所在显示器。
4. 矩形、椭圆、箭头、线、自由画笔、多行文字、递增序号、矩形马赛克、矩形模糊均渲染为真实输出像素。
5. 马赛克和模糊针对当前已合成图片采样，因此也处理之前绘制的标注。模糊为线性时间两遍滑动窗口方框滤波。
6. Ctrl+Z 撤销；Ctrl+Y / Ctrl+Shift+Z 重做。新编辑清除重做分支。最多保留 80 个可撤销操作，栅格历史约 128 MiB；更早命令合成到基础图片，保留视觉结果。单个大效果仍需容纳其自身像素缓冲。
7. Enter / Ctrl+C 复制，剪贴板占用时重试 4 次，失败显示提示并保留编辑。Ctrl+S 打开 PNG/JPEG 保存对话框，JPEG 质量为 94；取消保存保留编辑。图片先完整编码到同目录的唯一临时文件并刷新到磁盘，再通过同卷移动替换目标；编码或替换失败不会提前截断已有目标文件，清理仅针对本次创建的临时文件。保存异常显示原始原因。
8. 钉住创建独立置顶图片窗：拖动移动、滚轮缩放、双击复位、右键复制/关闭、Esc 关闭。初始窗口放在原选区所在显示器内。
9. 截图窗与贴图窗通过 Capture 的窗口排除 API 排除出后续捕获。

## 坐标与多显示器设计

- 依赖 App 清单中的 PerMonitorV2 DPI 感知。
- 鼠标读取原生 `GetCursorPos` 物理像素，减去虚拟桌面原点后才传给选择和绘制逻辑。屏幕原点为负时不丢失偏移。
- 覆盖窗通过原生 `SetWindowPos` 对齐整个虚拟桌面的物理矩形。WPF 渲染比例来自实际视口大小与物理截图大小之比；不使用跨屏 WPF 鼠标坐标推算物理像素。
- 导出通过 96 DPI `RenderTargetBitmap` 在最终选区的精确宽高绘制，不把显示器缩放写入图片。
- 选择框移动/缩放是在冻结桌面上重新裁剪，已有标注保持原来的桌面位置。

## 已实际执行的验证

```powershell
& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" build src\Cap.Annotation\Cap.Annotation.csproj
& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" run --project tests\Cap.Annotation.Tests\Cap.Annotation.Tests.csproj
```

Debug 和 Release 构建成功，0 警告，0 错误。测试成功，覆盖：反向拖拽、越界处理、移动不缩小、八个手柄命中及有效边界、负坐标、部分块马赛克平均、模糊边缘平均和单像素情况、输出精确尺寸和冻结状态、标注实际像素变化、撤销与重做逐字节一致、清除重做分支、80 操作上限与烘焙结果、各绘图工具实际渲染、效果处理先前标注。另验证 PNG 覆盖后可完整解码、JPEG 新建后可完整解码；锁定已有文件导致替换失败时，原始字节逐字节保持不变，成功/失败均不残留本次临时文件。

`git diff --check` 已执行，无报告。仓库基线文件均尚未跟踪，因此该命令不能验证新增文件与历史版本的语义差异；本模块新增文件已按职责逐个检查，未覆盖旧源码。

## 尚未实际验证与限制

- 未打开浏览器。
- 未在真实异构 DPI 双显示器、负原点三屏布局或屏幕旋转热插拔环境中做端到端人工验证；上述支持基于物理坐标实现与几何测试。截图流程进行中不会重新枚举屏幕布局，显示器热插拔应退出后重试。
- 剪贴板占用重试、系统保存对话框、实际点击拖拽和贴图操作尚需人工 UI 验证。栅格输出与内部状态已自动验证。
- 模糊与马赛克以矩形区域工作。颜色提供七个常用预设，粗细提供 2/3/5/8/12 像素。
- 截图没有额外的滚动捕获、OCR、标注对象逐个选中移动或矢量文件导出功能。
- 超大虚拟桌面截图与栅格效果需要与像素数成正比的内存；历史受限但当前画布、导出缓冲及单个效果的临时内存仍存在。
