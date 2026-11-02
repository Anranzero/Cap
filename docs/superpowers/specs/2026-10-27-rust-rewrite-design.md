# Cap Rust 重写设计

用户已批准继续按原生 Windows 路线完整重写，原 C# 源码和发布程序保留。2026-10-27 已明确批准 windows、serde/serde_json、flate2、quick-xml 生产依赖。

## 目标和约束

- 新工程只放在本项目 `rust/`，不改原 C# 业务逻辑，不提交用户既有暂存改动。
- Windows 10 2004+/11 x64；原生窗口、托盘、快捷键、单实例、浅深主题；保留现有图标和中文操作语义。
- 全部现有功能：截图选区/窗口识别、批注/历史/滤镜、PNG/JPG/剪贴板/贴图、滚动拼接、离线 Windows OCR、MP4/GIF 录制及暂停、鼠标/点击/坐标、预览、配置、自启动和中断恢复。
- 保留物理像素/负坐标、WGC/D3D11 优先与 GDI 回退、局部 GIF GDI、Media Foundation H.264 和流式 GIF；不增加录音或网络功能。
- 兼容 PascalCase settings.json 和 CAPJ0001 日志。测试使用独立配置、热键、文件和单实例名；不启用用户自启动，不强行关闭用户程序。
- 原程序与 Rust 程序不覆盖。输出 `rust/dist/cap-rust.exe`，不替换桌面入口。
- 源文件原则上低于 1000 行；职责拆分。不得以占位按钮、调用原 C# EXE 或隐藏失败代替实现。
- 不打开浏览器验收。原生自有测试窗口、合成像素和媒体解码可以用于验证；真实多屏/HDR/长时间场景需明确未覆盖范围。

## 方案

采用 Rust + windows 绑定调用 Win32/WinRT，替换 WPF/WinForms。相较保留 C# 界面，能够完成语言迁移；相较 WebView 路线，直接复用系统捕获、编码和绘图，避免增加网页界面运行时。标准库不足的 JSON/DEFLATE 使用已批准依赖，版本由 Cargo.lock 固定。

## 模块和接口

- `geometry`: `Rect { x:i32,y:i32,width:u32,height:u32 }`，排他右下边界，检查溢出；`Frame { width:u32,height:u32,pixels:Vec<u8> }` 顶向下紧密 BGRA32，放在 `pixels`。
- `annotation`：在 Frame 上绘制形状/文字/滤镜，撤销重做最多 80 次及约 128 MiB；UI 负责原生文字栅格化并交给图像模型。
- `scroll`：仅向下的保守拼接、重复/歧义拒绝、4000 万像素/30000 高度、撤销。
- `gif`：流式 GIF89a、RGB332、稳定抖动、LZW、变化矩形和重复帧延时合并。
- `capture`：窗口/显示器识别、GDI、WGC/D3D11、排除自有浮窗。
- `recording`/`recovery`：帧调度、暂停/停止、鼠标叠加、MF 编码、CAPJ0001 兼容和部分文件发布。
- `settings`/`platform`：类型化设置/校验/原子保存/备份、日志、热键、自启动和剪贴板。
- `app`/`ui`：原生窗口事件状态机，截图双工具栏、设置、录制控制和结果预览。

## 风险和验收

重点验证：负坐标与混合 DPI、COM 生命周期/目标关闭、重复停止/异常保存、恢复格式及剪贴板占用。先迁移算法并测试，再接 Windows 后端和交互；最终运行 cargo test、cargo fmt --check、cargo clippy、Release 构建、原生截图/MP4/GIF/OCR 冒烟。性能与体积用实际输出报告，不承诺语言迁移自动提高录制吞吐。
