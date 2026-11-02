# Rust 版为唯一发行版：剩余移植清单

.NET 版（`dist/cap.exe`）从此只作为参照，不再发布、不再出现在桌面上。
桌面唯一入口 `cap 截图` → `rust/dist/cap-rust.exe`（使用共享配置 `%LOCALAPPDATA%\cap\settings.json`）。

## Rust 已具备（核对过代码，与 .NET 对应）

| 能力 | Rust 位置 |
| --- | --- |
| 截图 / 批注 / 参数条 / 取色器 | `ui/selector.rs`、`ui/toolbar*.rs`、`ui/options_bar.rs`、`ui/magnifier.rs` |
| 滚动截图 + 拼接 + 预览 | `ui/scrolling.rs`（`UiEvent::PreviewImage`） |
| 录制 MP4 / GIF | `recording.rs`、`media.rs`、`ui/record_ui.rs`、`ui/record_setup.rs` |
| 录制崩溃恢复 | `app.rs`（恢复线程 + 预览） |
| 贴图 / 提取文字 / 预览窗口 | `ui/pin.rs`、`ui/ocr_card.rs`、`ui/preview.rs` |
| 设置窗口 / 托盘 / 热键 / 自启动 | `ui/settings*.rs`、`tray.rs`、`platform.rs` |
| 深浅主题 | `settings::Theme` 贯穿全部自绘 UI |

## 待移植 / 待补齐

### A. 录制性能（.NET 已完成，Rust 待移植）

- [ ] 恢复日志异步有界写入（Rust 当前恢复帧同步压缩）
- [ ] BGRA→NV12 并行转换
- [ ] 采集侧非阻塞回读 + 复用上一帧
- [ ] 编码器启用 `MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS`
- [ ] 采集/编码双线程流水线（有界队列）
- [ ] 码率改为 `宽×高×fps/10`（上限 30 Mbps）

### B. 飞书 UI 细节（parity-review.md 第 9 节）

- [ ] 标注工具语义：文本标注（≤16 字符 + 半透明底 + 不透明度）
- [ ] 复制后「保存到剪贴板」浮标
- [ ] 字号弹出下拉（现为步进切换）
- [ ] OCR 卡「编辑」内联可编辑
- [ ] 悬停窗口的独立尺寸标签

### C. 刻意不复刻（需你决定）

- 翻译：离线无后端，方案见对话记录（在线 API / 本地模型二选一）
- 录屏设备行（麦克风/摄像头）：需要音频与摄像头采集链路

## 验收方式

每项改动都必须：`cargo build --release` + `cargo clippy -D warnings` + `cargo test`（当前 134 项）
+ 真机实拍（`--profile` 隔离配置，避免污染日常设置），最后 `sync.ps1` 推送。
