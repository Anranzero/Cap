# Cap Rust 验证记录

## 实现范围

- `src/app.rs`、`main.rs`：单实例、配置迁移、托盘/快捷键/窗口事件、异步 OCR 与恢复、录制生命周期及退出清理。
- `platform.rs`、`tray.rs`：系统目录、配置事务的自启动/快捷键回退、图片/文字/文件剪贴板、原生保存对话框、托盘重建。
- `settings.rs`、`settings/`：原版 PascalCase JSON、完整 WPF 键名兼容、校验与原子写入；190 个原版合法键名的独立 fixture。
- `geometry.rs`、`pixels.rs`、`annotation.rs`、`scroll.rs`、`gif.rs`：物理坐标、图像处理、全部批注/历史、保守滚动拼接、流式 GIF。
- `capture/`、`media/`、`overlays.rs`、`recording.rs`、`recovery.rs`：WGC/D3D11/GDI、Windows PNG/JPEG/OCR/MP4、指针与点击/坐标、暂停与帧调度、CAPJ0001 原版恢复格式及取消。
- `ui/`、`selection.rs`、`text_raster.rs`：冻结选区、批注/文字、模式条、贴图、滚动、录制设置/控制、预览/定位、深浅主题和可缩放设置窗口。
- `app/smoke.rs`、`clipboard_smoke.rs`：显式隔离诊断。前者只使用独立 profile 与自有窗口；后者要求独立进程和私有窗口站，失败即停止。
- `Cargo.toml/lock`、`build.rs/ps1`、`resources/`：已批准并锁定的依赖、静态 CRT、原生图标/manifest 与构建门禁。没有调用原 C# 或引入 .NET、WebView、FFmpeg 运行时。

原 C# 和原发布文件保留，未暂存/提交用户已有改动。新 Rust 源码均低于 1000 行。

## 已实际验证

构建环境为 Rust MSVC 1.95、本地 Microsoft CRT/Windows SDK 和 Rust LLD，使用 `../../.build-tools/Enter-RustBuild.ps1`。实际执行：

```powershell
cargo test --locked --all-targets -- --test-threads=1
cargo test --locked --lib native_ocr_recognizes_owned_text -- --ignored --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
./build.ps1
```

解锁桌面后的全量结果：**58 library + 46 binary + 23 portable 测试执行通过，0 失败**。portable 复用算法测试，所以不是 127 个互不重复的场景。默认四个 ignore 中三个是由父测试带超时启动的子进程辅助入口，另一个为 OCR 环境测试；OCR 已明确单独执行并通过。

覆盖实际 GDI/WGC 像素、窗口放大/缩小补边/关闭、MP4 解码与时间、GIF LZW/抖动/时间、恢复截断/并发/取消/不覆盖已有输出、批注透明度和历史预算、手动/自动滚动及撤销/保守结束、贴图、预览定位、录制参数与边框、WPF 配置迁移、短窗滚动布局、文字键盘、Tab 导航、鼠标捕获中断及托盘消息恢复。

独立审查发现的问题均已修复并定向复查。关键问题均保留实际 RED→GREEN 记录，见各专项报告。审查没有替代实际测试。

`--smoke-test <空目录>` 的主程序诊断已通过：鼠标选区与矩形批注导出、PNG 像素往返、深浅主题及隔离配置保存、取消选区、GIF/GDI 与 MP4/WGC 开始/暂停/继续/结束/保存/预览、解码像素、录制中退出等待保存。检查前后旧版/Rust 用户配置与 Run 自启动值一致，用户剪贴板未操作。已查看深浅设置窗口截图。

## 环境问题与未验证部分

初轮测试读到 Windows 锁屏：1 个捕获和 7 个滚动像素测试失败，主程序 smoke 也失败。用户解锁后串行复跑全部通过；没有修改捕获算法来绕过环境验证。

剪贴板独立 consumer 诊断使用唯一私有 station/desktop，所有访问前检查身份，不回退到 WinSta0。主机在创建窗口站时返回 **0x80070005**，最小权限重试仍被拒绝，尚未实际执行 CF_BITMAP/Unicode/HDROP consumer 验收。没有访问或清空用户剪贴板；不能声称第三方应用粘贴已验证。现有 UI 测试只验证正确像素/路径事件，剪贴板内存布局与所有权目前由代码审查确认。

真实自启动开关、正式全局快捷键冲突、真实 Explorer 进程重启、系统 Save As/第三方粘贴、不同显卡/混合 DPI 和长时间录制没有全部自动端到端覆盖。Windows 驱动及系统 COM 调用自身没有硬超时保证。真实用户配置未作为测试目标。

算法独立复查未发现正常截图/录制调用链中的新增阻塞问题。代码阅读确认三个公共 API 边界：透明底图上的滤镜 Stamp 重合成会增加 alpha、文字栅格忽略传入颜色 alpha、不连续 GIF 时间戳的间隙归到后一帧。生产入口使用不透明截图/颜色和连续录制时间轴；GIF 间隙行为继承原 C#。此次未扩大修改这些边界，也未将其描述为额外已验证能力。

## 发布记录

最终文件：`dist/cap-rust.exe`，**1,175,040 字节**。SHA256：

`DC39B97C883A96CF9D7E8ACA459A823920C716812936AEBA86FD5F4909E8B3A7`

`./build.ps1` 全门禁成功，日志为 `.artifacts/final-build.log`。其后针对诊断失败时不写真实用户日志的小改动，再次执行格式、严格 Clippy、Release 构建成功。最终发布副本的 `--smoke-test dist/smoke` 退出码 **0**，七项主程序检查全部通过，JSON 为 `dist/smoke/smoke-result.json`；输出包含深浅截图、批注 PNG、GIF、MP4 和录制中退出保存的 GIF。

直接解析最终 PE64：GUI 子系统，CLR 目录为 0，所有导入来自 Windows 系统 DLL，无 mscoree/VCRUNTIME/MSVCP/UCRTBASE 外部运行时。记录在 `.artifacts/final-pe.json`。共享工具链 helper 反复加载不会重复追加静态 CRT 参数，已实际执行两次加载比较确认。

原版 `../dist/cap.exe` SHA256 保持 `5FFA2AF3F58686D10B5249A2FCF4E7747E37FD59140A17D6092539C576728A58`；桌面独显管家原 EXE 保持 `F24A03AA00E638716DA1B2A4EF32AA65BA4ED31520977BD7CD8E32FFB26D6291`。最终发布文件没有替换原程序或桌面入口。

## 交付后修复：GameViewer 中选区不可见

用户确认 Ctrl+Alt+A 会接管输入、Esc 会退出，但没有显示蒙版。主机同时运行 GameViewer 和其虚拟显示器。原实现为冻结选区及工具条设置 `WDA_EXCLUDEFROMCAPTURE`；[Windows 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)说明该标志会使窗口从捕获画面中消失。这与远程端看不见选区、却仍被选区接收输入的现象吻合。

实际改动仅为 `ui/selector.rs` 和 `ui/toolbar.rs` 取消这两类冻结截图窗口的捕获排除。底图先于窗口创建完成，导出仍读取 AnnotationDocument，不依赖捕获当前蒙版。录制控制条、滚动控制和贴图的排除策略未扩大修改。

新增 `selector_tests.rs` 回归：旧实现实际 RED，Windows 返回 affinity=17，而要求为0；修复后该属性检查通过。锁屏期间屏幕像素验证被阻塞；用户解锁后，实际桌面捕获中的蒙版像素、两个工具条的可见属性及导出原始像素检查全部通过。用户也确认 GameViewer 中已经能看到蒙版，随后报告鼠标移动闪烁，见下一节。

此前另新增 `app/smoke.rs` 中的触发回归：独立实例实际注册临时快捷键，投递 WM_HOTKEY 及模拟托盘截图命令后，全虚拟桌面选区创建、显示、范围及关闭检查通过。该测试不等于真实硬件按键或远程客户端端到端验收。

修复后的格式、严格全目标 Clippy、Release 构建通过。原 Cap 进程已用不带 `/F` 的正常退出信号结束，旧 EXE 保存在 `.artifacts/remote-overlay-before-20261027/cap-rust.exe`，新 EXE 已替换同一快捷方式目标并启动设置窗口。该轮发布 SHA256：`5A29F60BC3EC580EAA7B34641B5229238C1ECDFEA288CF117B4C97AF43CF94FE`。解锁后的48项界面测试、发布版七项自检均通过。

完整库回归另有既有 WGC resize 用例失败：预期窗口位置180，实际270。实际结果为57 passed、1 failed、4 ignored；normal/closed WGC 场景已通过。根据代码阅读，测试窗口由 PMv2 工作线程创建，但测试辅助函数 move_resize 在未设置 DPI 的调用线程执行 SetWindowPos，150% 缩放将180换算成270。该问题位于已有测试辅助窗口，本次没有扩大修改捕获后端或测试辅助函数；不能声称这一轮完整库门禁通过。日志为 `.artifacts/remote-overlay-final-tests.log`。

## 交付后修复：鼠标移动闪烁

原选区每次 hover 鼠标移动都无条件使整张桌面失效，并直接向窗口 DC 先绘制暗色底图、再绘制亮色选区与标记，远程捕获可能看到中间绘制过程。

- `ui/selector.rs`：仅 hover/拖动矩形实际变化时安排重绘；使用缓存位图完成整帧，再一次 BitBlt 到窗口。
- `ui/selector_buffer.rs`：私有 GDI 缓存模块；同尺寸复用，失败返回真实错误，退出时还原并释放位图/DC。
- `ui/selector_tests.rs`：同一 hover 区域移动不再产生绘制失效区。旧实现实际以 `unchanged hover must not schedule a full desktop repaint` 失败，修复后通过。
- 缓存原生测试验证分段绘制不改变目标图像，只有 present 后目标才出现完整帧，且第二帧不会提前显示。

实际执行 `cargo test --locked --bin cap-rust -- --test-threads=1`：**50 passed、0 failed**；格式检查、严格全目标 Clippy、Release 构建均通过。Release 自检七项通过，报告为 `.artifacts/selector-flicker-release-smoke-20261027-201224/smoke-result.json`，没有写用户配置/剪贴板/自启动。已检查实际差异，相关源码均低于1000行。

旧 EXE 备份在 `.artifacts/selector-flicker-before-20261027/cap-rust.exe`。关闭设置窗口会留在托盘，因此第一次普通关闭未退出、替换及时停止；随后仅对已核对 PID/类名/标题的 Cap 后台窗口发送 WM_CLOSE，程序正常退出后才替换。最新发布文件已启动，SHA256 为 `FE47BDD7906DA7088E3E90C872733BB14C2D08AB25A7C9EDCA24FBDA8AD28878`。GameViewer 客户端的闪烁改善仍等待用户实际确认；缓存增加一张选区尺寸的位图内存。

旧实例退出期间出现三条“释放快捷键失败：无效的窗口句柄”日志。代码阅读表明 Quit 分支可能先销毁后台窗口，再由 Hotkeys 的 Drop 执行清理；该清理顺序问题与此次绘制修改无关，新实例没有出现注册失败日志。此次未扩大修改退出逻辑，后续可单独处理日志与清理顺序。

## 2026-10-28：恢复可识别的截图工具栏

用户指出工具栏只显示一排单字、难以识别。对照原版 `EditorToolbar.cs`、`CaptureModeBar.cs` 和 `Shared/VectorIcon.cs`，恢复19个既有工具单元的24单位线框图标语义，使用系统 GDI+ 抗锯齿绘制；完成/取消分别为绿色勾和红色叉，颜色按钮展示当前色块，增加分隔线与悬停反馈。模式栏与提示使用 Microsoft YaHei UI 字体。

实际文件：`ui/toolbar.rs`、新增的 `toolbar_icons.rs`、`toolbar_native.rs`、`toolbar_tests.rs`；`selector.rs` 和 `selector_buffer.rs` 仅放宽已有离屏缓存的 UI 内部可见性，让工具栏也能整帧提交。没有新增依赖，没有修改截图/批注的命令编号和功能映射。

先运行原生提示回归，旧实现实际 RED：找不到工具提示窗口（0，而要求1）。完成后验证19个工具提示和4个模式提示的数量/完整文字、17个直接命令及4个模式命令的路由、远程捕获 affinity=0、同一按钮内移动不重复安排重绘。颜色/粗细的原有弹出菜单实现保持，未自动点选其菜单项。

原生 Common Controls 支持版本化结构大小；为兼容本项目未启用v6控件主题的 manifest，提示使用 TTTOOLINFOW 的V2前缀，稳定UTF-16文字由工具栏资源对象持有，销毁提示窗口后才释放。参考 [Microsoft Common Control Versions](https://learn.microsoft.com/en-us/windows/win32/controls/common-control-versions) 和 [TTTOOLINFOW](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/ns-commctrl-tttoolinfow)。

实际执行：格式检查、`cargo test --locked --bin cap-rust -- --test-threads=1`（**52 passed、0 failed**）、严格全目标Clippy、Release构建均通过。新发布版七项主程序自检通过，JSON为 `.artifacts/toolbar-icons-release-smoke-20261028-180042/smoke-result.json`。已查看程序真实绘制的 `tools.png` 与 `modes.png`，位于 `.artifacts/toolbar-visual-1793181300111331100/`；图标、模式文字与完成/取消颜色均可辨识。本轮没有重跑已知失败的库DPI辅助测试，也不将其描述为已修复。

已备份当前源码及旧EXE到 `.artifacts/toolbar-icons-before-20261028/`，检查实际差异与UTF-8内容；相关源文件均低于1000行。只向核对过PID/类名/标题的Cap后台窗口发送正常退出消息，随后替换原快捷方式目标并重启。最新文件大小 **1,191,424 字节**，SHA256：`EC6421444100FC9F5DEC88A956083AD5480D0D9B3BDA0BACD678BB8DF75565AC`。真实远程客户端的悬停提示与交互体验仍以用户使用反馈为准。

## 2026-10-29：替换用户指定的应用图标

`resources/cap-master.png` 原样保存本次用户提供的1254×1254透明PNG，SHA256为 `6CBD0436A62244B058A189F1B90278D72C9998CD7CDA84334B60F6E62B250DCF`。`resources/cap.ico` 更新为16、20、24、32、40、48、64、128、256像素的九帧32位透明图标，`resources/cap.png` 为256像素预览。沿用现有资产工具的可见内容边界算法，排除透明留白与孤立杂点，内容范围为 `(196,203,862,845)`，等比居中占画布96%，保留原图案和配色。

当前环境缺少原资产工具所需的.NET SDK及Windows Desktop 8运行时，因此没有改动工具项目或安装依赖；转换使用本机已有的System.Drawing高质量缩放与PNG编码。程序继续通过既有 `build.rs` 将图标嵌入资源ID 1，窗口与托盘沿用既有加载方式，没有修改Rust源码或生产依赖。

实际验证：九帧ICO均能解码，尺寸正确且四角透明；执行 `cargo build --locked --release` 成功。通过Windows资源API逐帧读取新EXE中的图标，九帧PNG哈希均与ICO完全一致，Windows原生加载16、32、48、256像素图标成功。已查看256像素嵌入图标和运行中窗口实际加载的图标。对比修改前保存的源码哈希，`src/` 内全部文件保持一致；此次为资源替换，未新增测试或重跑功能测试。

发布文件为 `dist/cap-rust.exe`，**1,209,344字节**，SHA256为 `39FC64C389BBB3E761277F82B2725BBDDD1E3A45418DB9388D9B60C9408DF14B`，已核对与Release构建产物一致。桌面 `Cap Rust.lnk` 仍指向该EXE，图标位置更新为本项目的 `resources/cap.ico`，并已发送Explorer刷新通知。新程序已启动并确认后台窗口加载了图标；未通过桌面截图确认Explorer最终显示效果。

旧ICO、EXE、快捷方式及本报告备份位于 `.artifacts/icon-replacement-20261029/`；该目录也保存逐帧检查、原生图标PNG及部署记录。本次没有改变截图、录屏、快捷键或配置逻辑。

## 2026-10-29：界面使用华为鸿蒙字体

字体注册表中有HarmonyOS Sans SC条目，但对应字体文件已丢失；修改前通过GDI实际查询，请求该字体时得到“宋体”。从[华为官方字体包](https://developer.huawei.com/images/download/general/HarmonyOS-Sans.zip)提取原始Regular字体与完整许可，保存于 `resources/fonts/`，逐字节哈希与下载包内原件一致。来源、哈希和版权信息见该目录的README。没有安装系统字体或新增生产依赖。

实际源文件改动：

- 新增 `ui/fonts.rs`，将字体与许可嵌入EXE，用AddFontMemResourceEx在Cap进程内加载；统一管理16像素常规字体和14像素紧凑字体，先释放字体句柄再释放字体资源。
- `ui/mod.rs`、`runtime.rs` 注册模块并在创建窗口前初始化字体；`common.rs` 的自绘文字和原生子控件使用共享字体。
- `toolbar_native.rs` 的模式文字与悬停提示使用共享紧凑字体，窗口不再单独销毁共享字体句柄。
- `settings.rs` 的关于页补充字体名称、版权声明和可滚动的完整许可文本。
- `settings_tests.rs` 新增实际字体与文字边界回归；`toolbar_tests.rs` 增加工具提示实际字体检查。

新增回归在旧实现实际失败：控件1实际使用“宋体”，而要求为“HarmonyOS Sans SC”；完成后实际字体名称、标签宽高与工具提示字体均通过。执行 `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked --bin cap-rust -- --test-threads=1`（**53 passed，0 failed**）及Release构建均成功。首次Clippy发现输出参数引用形式警告，改用可变裸指针后完整复跑上述检查通过。

发布候选的隔离 `--smoke-test` 七项检查通过，报告为 `.artifacts/harmony-font-20261029/release-smoke/smoke-result.json`。其设置页屏幕截图被其他前台窗口遮挡，未作为视觉通过证据；改用已核对进程身份的正式Cap设置窗口，通过PrintWindow直接导出客户区，并在辅助线程临时启用PMv2以避免截图尺寸被DPI虚拟化。已查看完整878×624图像 `.artifacts/harmony-font-20261029/settings-native-print-full.png`，常规设置页文字与按钮完整，使用无衬线鸿蒙字体。其他页面由现有短窗口、滚动及导航测试覆盖，没有逐页人工验收；未重跑前述已知失败的库DPI辅助测试。

已正常退出旧Cap并更新原桌面快捷方式目标，启动新设置窗口，用户配置文件哈希保持一致。最新EXE **9,505,280字节**，SHA256为 `A5500BF81D8AEA3834F15B8B84FE328708F520E8ACEB8AE64140C4F913671C31`；完整字体内置使文件增加约8.3MB。修改前源码、EXE、RED/GREEN记录、检查日志与部署记录位于 `.artifacts/harmony-font-20261029/`。已检查实际差异，只有上述界面字体及其许可展示变化，截图和录制后端未修改。
