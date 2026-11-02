using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using Cap.Annotation;
using Cap.Capture;
using Cap.Core;
using Cap.Recording;
using Cap.Platform;
using Forms = global::System.Windows.Forms;

namespace Cap.App;

public partial class App : Application
{
    private readonly SettingsStore store = new();
    private AppSettings settings = new();
    private Mutex? instance;
    private HotkeyService? hotkeys;
    private Forms.NotifyIcon? tray;
    private SettingsWindow? settingsWindow;
    private RecordingSession? session;
    private RecordingControlWindow? controls;
    private bool busy, exiting, restoreSettingsAfterRecording;
    private Task? stopTask;
    private CancellationTokenSource? recoveryCancellation;
    private Task<RecordingResult>? recoveryTask;

    protected override async void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        DispatcherUnhandledException += (_, args) => { AppLog.Error(args.Exception); args.Handled = true; Ui.Error(null, args.Exception); };
        TaskScheduler.UnobservedTaskException += (_, args) => { AppLog.Error(args.Exception); args.SetObserved(); };
        if (e.Args.Contains("--smoke-test")) { await AppSmokeTest.RunAsync(e.Args); Shutdown(); return; }
        instance = new Mutex(true, @"Local\cap.desktop.8E306F42", out bool first);
        if (!first)
        {
            if (!e.Args.Contains("--startup"))
                MessageBox.Show($"cap 已在后台运行。\n请先从托盘右键菜单退出；旧版也可进入设置，点击底部“退出程序”。\n再双击桌面“cap 测试版”打开本次版本 v{typeof(App).Assembly.GetName().Version!.ToString(3)}。", "cap 已在运行", MessageBoxButton.OK, MessageBoxImage.Information);
            Shutdown(); return;
        }
        settings = store.Load(); Ui.Theme(settings.Theme == "Dark");
        CreateTray();
        hotkeys = new HotkeyService(); hotkeys.Activated += command => _ = ExecuteAsync(command);
        hotkeys.CaptureGesture = gesture => settingsWindow?.CaptureHotkey(gesture) == true;
        if (!hotkeys.TryRegister(settings, out string? error))
        {
            // A silent toast is easy to miss; keep the state visible in the tray until fixed.
            Notify("快捷键注册失败", error ?? "请在设置中更换快捷键。");
            if (tray != null) tray.Text = "cap · 快捷键不可用（托盘右键可重试）";
        }
        if (store.LoadWarning != null) Notify("设置已重置", store.LoadWarning);
        if (settings.RunAtStartup) { try { StartupService.SetEnabled(true); } catch (Exception ex) { AppLog.Error(ex); Notify("自启动未更新", ex.Message); } }
        if (e.Args.Contains("--settings")) ShowSettings();
    }
    private void CreateTray()
    {
        var menu = new Forms.ContextMenuStrip();
        menu.Items.Add("截屏", null, (_, _) => _ = ExecuteAsync(AppCommand.Screenshot));
        menu.Items.Add("录制", null, (_, _) => _ = ExecuteAsync(AppCommand.Recording));
        menu.Items.Add("重新注册快捷键", null, (_, _) => RetryHotkeys());
        menu.Items.Add("设置", null, (_, _) => ShowSettings());
        menu.Items.Add("退出", null, (_, _) => _ = ExitAsync());
        tray = new Forms.NotifyIcon { Text = "cap · 截图 / 录屏", Icon = TrayIcon.Create(), ContextMenuStrip = menu, Visible = true };
        tray.DoubleClick += (_, _) => _ = ExecuteAsync(AppCommand.Screenshot);
    }
    /// <summary>Retries hotkey registration, e.g. after another app released the combination.</summary>
    private void RetryHotkeys()
    {
        if (hotkeys == null) return;
        if (hotkeys.TryRegister(settings, out string? error))
        {
            if (tray != null) tray.Text = "cap · 截图 / 录屏";
            Notify("快捷键已恢复", "截图 / 录制快捷键现在可以正常使用。");
        }
        else
        {
            Notify("仍然失败", error ?? "请在设置中更换快捷键。");
        }
    }
    private async Task ExecuteAsync(AppCommand command)
    {
        if (session != null) { if (command != AppCommand.Screenshot) await StopRecordingAsync(); return; }
        if (busy || exiting) return;
        busy = true;
        bool settingsWereVisible = settingsWindow?.IsVisible == true;
        Window? resultWindow = null;
        try
        {
            if (!CaptureService.IncludeOwnWindows) settingsWindow?.Hide();
            var initialMode = command == AppCommand.Screenshot ? CaptureMode.Screenshot : CaptureMode.Recording;
            var screenshot = await ScreenshotWorkflow.RunAsync(settings.SaveDirectory, settings.Theme == "Dark", initialMode);
            if (screenshot == null) return;
            switch (screenshot.Mode)
            {
                case CaptureMode.Recording:
                    restoreSettingsAfterRecording = settingsWereVisible;
                    StartRegionRecording(screenshot.Bounds, command == AppCommand.Gif ? RecordingFormat.Gif : screenshot.RecordingFormat);
                    break;
                case CaptureMode.Text:
                    string text = await OcrService.ExtractTextAsync(screenshot.Image);
                    if (!exiting) await CopyExtractedTextAsync(text);
                    break;
                case CaptureMode.Scrolling:
                    // In-place scrolling capture already returned the stitched image with the closed overlay.
                    if (screenshot.ScrollImage is { } stitched) { resultWindow = new ImageResultWindow(stitched, settings.SaveDirectory); resultWindow.Show(); }
                    break;
            }
        }
        catch (Exception ex) { if (session == null) restoreSettingsAfterRecording = false; Ui.Error(null, ex); }
        finally
        {
            busy = false;
            if (settingsWereVisible && session == null && !exiting)
            {
                settingsWindow?.Show();
                if (resultWindow?.IsVisible == true) resultWindow.Activate();
                else settingsWindow?.Activate();
            }
        }
    }
    private async Task CopyExtractedTextAsync(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            Notify("未识别到文字", "请重新框选清晰的文字区域。");
            return;
        }
        const int attempts = 4;
        for (int attempt = 0; attempt < attempts; attempt++)
        {
            if (exiting) return;
            try
            {
                Clipboard.SetText(text);
                Notify("文字已复制", "可直接粘贴使用。");
                return;
            }
            catch (ExternalException error)
            {
                if (attempt == attempts - 1)
                    throw new InvalidOperationException("剪贴板暂时不可用，文字未复制。请稍后重新提取。", error);
                await Task.Delay(80);
            }
        }
    }
    private void StartRegionRecording(PixelRect region, RecordingFormat format)
    {
        StartRecording(new RecordingOptions
        {
            Bounds = region, WindowHandle = 0, Format = format,
            FramesPerSecond = format == RecordingFormat.Gif ? settings.GifFramesPerSecond : settings.FramesPerSecond,
            OutputWidth = format == RecordingFormat.Gif ? settings.GifMaxWidth : 0,
            ShowCursor = settings.ShowCursor, HighlightClicks = settings.HighlightClicks, ShowCoordinates = settings.ShowCoordinates,
            RelativeCoordinates = settings.RelativeCoordinates, CoordinateFontSize = settings.CoordinateFontSize, CoordinateColor = settings.CoordinateColor,
            OutputPath = NewRecordingPath(format)
        });
    }
    private string NewRecordingPath(RecordingFormat format)
    {
        Directory.CreateDirectory(settings.SaveDirectory);
        return Path.Combine(settings.SaveDirectory, "cap-" + DateTime.Now.ToString("yyyyMMdd-HHmmss-fff") + (format == RecordingFormat.Gif ? ".gif" : ".mp4"));
    }
    private async Task ConfigureRecordingAsync()
    {
        if (busy || session != null || exiting) return;
        busy = true;
        bool settingsWereVisible = settingsWindow?.IsVisible == true;
        try
        {
            if (!CaptureService.IncludeOwnWindows) settingsWindow?.Hide();
            var setup = new RecordingSetupWindow(settings, RecordingFormat.Mp4);
            if (setup.ShowDialog() != true || setup.Options == null || exiting) return;
            var options = setup.Options;
            if (setup.SelectRegion)
            {
                PixelRect? region = await ScreenshotWorkflow.SelectRegionAsync(settings.Theme == "Dark");
                if (region == null || exiting) return;
                options = options with { Bounds = region.Value, WindowHandle = 0 };
            }
            restoreSettingsAfterRecording = settingsWereVisible;
            StartRecording(options with { OutputPath = NewRecordingPath(options.Format) });
        }
        catch (Exception ex) { if (session == null) restoreSettingsAfterRecording = false; Ui.Error(null, ex); }
        finally
        {
            busy = false;
            if (settingsWereVisible && session == null && !exiting) { settingsWindow?.Show(); settingsWindow?.Activate(); }
        }
    }
    private void StartRecording(RecordingOptions options)
    {
        var recording = new RecordingSession(options); session = recording;
        controls = new RecordingControlWindow(options);
        controls.PauseChanged += paused => { try { if (paused) recording.Pause(); else recording.Resume(); } catch (Exception ex) { Ui.Error(controls, ex); } };
        controls.StopRequested += () => _ = StopRecordingAsync();
        recording.Progress += (_, progress) => Dispatcher.BeginInvoke(() => { if (ReferenceEquals(session, recording)) controls?.Update(progress); });
        try { recording.Start(); controls.Show(); _ = ObserveCompletionAsync(recording); if (tray != null) tray.Text = "cap · 正在录制（再次按快捷键停止）"; }
        catch { controls.CloseAfterStop(); controls = null; session = null; recording.Dispose(); throw; }
    }
    private async Task ObserveCompletionAsync(RecordingSession recording)
    {
        Exception? failure = null;
        try { await recording.Completion; }
        catch (Exception ex) { failure = ex; }
        if (ReferenceEquals(session, recording)) await StopRecordingAsync();
        else if (failure != null) AppLog.Error(failure);
    }
    private Task StopRecordingAsync()
    {
        if (stopTask != null) return stopTask;
        if (session == null) return Task.CompletedTask;
        return stopTask = StopRecordingCoreAsync(session);
    }
    private async Task StopRecordingCoreAsync(RecordingSession recording)
    {
        // Yield before completion so every concurrent stop/exit observes the same task.
        await Task.Yield();
        PreviewWindow? preview = null;
        try
        {
            controls?.BeginStopping();
            if (tray != null) tray.Text = "cap · 正在保存录制…";
            var result = await recording.StopAsync();
            controls?.CloseAfterStop(); controls = null;
            if (!exiting) { preview = new PreviewWindow(result); preview.Show(); }
        }
        catch (Exception ex) { Ui.Error(controls, ex); }
        finally
        {
            controls?.CloseAfterStop(); controls = null; recording.Dispose(); session = null; stopTask = null;
            if (tray != null) tray.Text = "cap · 截图 / 录屏";
            if (restoreSettingsAfterRecording && !exiting)
            {
                settingsWindow?.Show();
                if (preview?.IsVisible == true) preview.Activate();
                else settingsWindow?.Activate();
            }
            restoreSettingsAfterRecording = false;
        }
    }
    private void ShowSettings()
    {
        if (settingsWindow != null) { settingsWindow.Show(); settingsWindow.Activate(); return; }
        settingsWindow = new SettingsWindow(settings, SaveSettings);
        settingsWindow.ThemeChangeRequested += SaveTheme;
        settingsWindow.ExitRequested += () => _ = ExitAsync();
        settingsWindow.RecoveryRequested += () => _ = RecoverAsync();
        settingsWindow.AdvancedRecordingRequested += () => _ = ConfigureRecordingAsync();
        settingsWindow.Closed += (_, _) => { settingsWindow = null; if (!settings.CloseToTray && !exiting) _ = ExitAsync(); };
        settingsWindow.Show();
    }
    private void SaveTheme(string theme)
    {
        var next = settings with { Theme = theme };
        store.Save(next);
        settings = next;
    }
    private bool SaveSettings(AppSettings next)
    {
        var previous = settings;
        if (hotkeys != null && !hotkeys.TryRegister(next, out string? error)) { Ui.Error(settingsWindow, new InvalidOperationException(error)); return false; }
        try
        {
            if (next.RunAtStartup != previous.RunAtStartup) StartupService.SetEnabled(next.RunAtStartup);
            store.Save(next); settings = next; Ui.Theme(next.Theme == "Dark"); return true;
        }
        catch (Exception ex)
        {
            var errors = new List<Exception> { ex };
            try
            {
                if (hotkeys != null && !hotkeys.TryRegister(previous, out string? rollbackError))
                    errors.Add(new InvalidOperationException("原快捷键恢复失败：" + rollbackError));
            }
            catch (Exception rollback) { errors.Add(new InvalidOperationException("原快捷键恢复失败：" + rollback.Message, rollback)); }
            if (next.RunAtStartup != previous.RunAtStartup)
            {
                try { StartupService.SetEnabled(previous.RunAtStartup); }
                catch (Exception rollback) { errors.Add(new InvalidOperationException("开机自启动设置恢复失败：" + rollback.Message, rollback)); }
            }
            Ui.Error(settingsWindow, errors.Count == 1 ? ex : new AggregateException("保存设置失败，且部分设置未能恢复。请检查快捷键和开机自启动后重新保存。", errors));
            return false;
        }
    }
    private async Task RecoverAsync()
    {
        if (busy || session != null) return; busy = true;
        try
        {
            var candidates = RecordingRecovery.FindCandidates(settings.SaveDirectory);
            if (candidates.Count == 0) { MessageBox.Show("当前保存目录中没有可恢复的录制。", "cap"); return; }
            var picker = new Microsoft.Win32.OpenFileDialog { Title = "选择要恢复的 cap 录制日志", InitialDirectory = settings.SaveDirectory, Filter = "cap 录制恢复文件|*.capjournal|所有文件|*.*", FileName = candidates[0].JournalPath };
            if (picker.ShowDialog() != true) return;
            var selectedCandidates = RecordingRecovery.FindCandidates(Path.GetDirectoryName(picker.FileName)!);
            var candidate = selectedCandidates.FirstOrDefault(c => string.Equals(c.JournalPath, picker.FileName, StringComparison.OrdinalIgnoreCase))
                ?? throw new InvalidDataException("选择的文件不是可恢复的 cap 录制日志。");
            var format = candidate.Format;
            string output = Path.Combine(settings.SaveDirectory, "cap-recovered-" + DateTime.Now.ToString("yyyyMMdd-HHmmss") + (format == RecordingFormat.Gif ? ".gif" : ".mp4"));
            Notify("正在恢复录制", "恢复完成后将打开预览。较长录制可能需要一些时间。");
            recoveryCancellation = new CancellationTokenSource();
            recoveryTask = RecordingRecovery.RecoverAsync(picker.FileName, output, recoveryCancellation.Token);
            var result = await recoveryTask;
            if (!exiting) new PreviewWindow(result).Show();
        }
        catch (OperationCanceledException) { if (!exiting) Notify("恢复已取消", "原始恢复日志已保留。"); }
        catch (Exception ex) { if (!exiting) Ui.Error(null, ex); else AppLog.Error(ex); }
        finally { recoveryTask = null; recoveryCancellation?.Dispose(); recoveryCancellation = null; busy = false; }
    }
    private void Notify(string title, string text) { tray?.ShowBalloonTip(3500, title, text, Forms.ToolTipIcon.Info); }
    private async Task ExitAsync()
    {
        if (exiting) return;
        if (session != null && MessageBox.Show("停止录制、保存文件并退出 cap？", "cap", MessageBoxButton.YesNo, MessageBoxImage.Question) != MessageBoxResult.Yes) return;
        exiting = true;
        recoveryCancellation?.Cancel();
        var recovering = recoveryTask;
        if (recovering != null) { try { await recovering; } catch (Exception ex) { AppLog.Error(ex); } }
        await StopRecordingAsync(); Shutdown();
    }
    protected override void OnExit(ExitEventArgs e)
    {
        hotkeys?.Dispose(); if (tray != null) { tray.Visible = false; tray.Icon?.Dispose(); tray.Dispose(); }
        session?.Dispose(); instance?.Dispose(); base.OnExit(e);
    }
}
