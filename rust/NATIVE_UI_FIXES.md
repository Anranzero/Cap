# 原生文本输入与托盘恢复验证

验证日期：2026-10-27。

## 修改范围

- `src/ui/selector.rs`：多行 EDIT 增加 `ES_WANTRETURN`；普通 Enter、Shift+Enter 保留给编辑框，只有 Ctrl+Enter 提交文字，Esc 取消当前文字。投递文字操作失败时进入现有错误事件通道。显式忽略四处原有 GDI 返回值，消除编译警告。
- `src/ui/selector_tests.rs`：使用本测试创建的原生窗口和生产 Selector，向真实 EDIT 注入字符与换行，验证多行内容、Ctrl+Enter 添加批注、Esc 取消后仍保留选区窗口。键盘状态只通过当前线程的 `SetKeyboardState` 设置，退出时恢复。
- `src/ui/runtime.rs`：根窗口创建时注册 `TaskbarCreated`，收到该消息后发出 `ShellRestart` 事件；保持 `RootHandler` 为 unit struct。
- `src/tray.rs`：`restore()` 使用现有窗口、图标和回调信息重新添加托盘图标；已存在时回退到更新。两次 Shell 调用均失败时报告调用结果和各自的 Win32 错误码，清零旧错误码以避免误报历史错误。
- `src/ui/mod.rs`：增加 `ShellRestart`，按主代理要求删除未使用的 `OpenPreview` 事件。App 的对应事件处理由主代理修改。

## 已执行验证

构建环境：先 dot-source `D:\Desktop\项目\.build-tools\Enter-RustBuild.ps1`。

```powershell
cargo test --bin cap-rust ui::selector::tests -- --test-threads=1
cargo test --bin cap-rust ui::runtime::tests -- --test-threads=1
cargo test --bin cap-rust tray::tests -- --test-threads=1
```

结果：4 个原生回归测试通过。测试前实际观察到普通 Enter 被吞掉的回归用例失败，以及投递 `TaskbarCreated` 后事件队列为空的回归用例失败；修复后两项通过。

托盘测试仅操作本测试隐藏窗口所持有的托盘图标：首次恢复、重复恢复、删除该图标、重新恢复、退出时清理均成功。消息测试使用 `PostMessageW` 向自建根窗口投递真实注册消息，并验证产出的 `ShellRestart` 事件。

`cargo test -- --list` 成功列举整个测试集；本分工仅运行上面三个定向命令，全量串行测试由主代理收口，以免多组原生窗口争用焦点或影响截图验证。

## 差异与验证边界

已将四个原文件备份到 `.artifacts/native-ui-backup-20261027-161949`，并逐文件检查实际差异。所有文件保持 UTF-8、LF，无替换字符，无整文件换行变化；各源码文件均小于 1000 行。

本次验证没有打开浏览器、操作用户剪贴板或注册表，也没有重启 Explorer。真实 Explorer 进程重启后的端到端恢复未执行；已分别验证同一注册消息的原生窗口路由以及图标丢失后的 Shell 重新添加行为。Shell 服务整体不可用时的失败路径未通过故障注入验证。

## 最终复查：鼠标捕获中断

独立复查发现拖选/批注在失去捕获后仍保留临时状态。新增自有窗口夺取 SetCapture 的实际回归，旧实现以 `lost capture must stop region movement` 失败；修复后通过。WM_CAPTURECHANGED / WM_CANCELMODE 清理 drag、paint_start 与 points，正常鼠标松开先完成提交再释放捕获。最终定向复查已关闭此意见，未发现剩余代码问题。该回归与正常批注导出均由主代理实际验证。
