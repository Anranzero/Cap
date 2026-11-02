using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using Cap.Platform;

/// <summary>Runs the real application UI with isolated preview settings and shortcuts.</summary>
internal static class DeveloperPreview
{
    internal static int Run(Cap.App.App app, string output)
    {
        Cap.Capture.CaptureService.IncludeOwnWindows = true;
        Directory.CreateDirectory(output);
        var store = new SettingsStore(Path.Combine(output, "settings"));
        var config = store.Load() with { RunAtStartup = false, CloseToTray = false };
        if (!File.Exists(store.FilePath)) config = config with
        {
            SaveDirectory = Path.Combine(output, "captures"), Theme = "Light",
            ScreenshotHotkey = "Ctrl+Shift+F10", RecordingHotkey = "Ctrl+Shift+F11", GifHotkey = "Ctrl+Shift+F12"
        };
        Type type = typeof(Cap.App.App);
        void Apply(AppSettings settings)
        {
            type.GetField("settings", BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(app, settings);
            type.Assembly.GetType("Cap.App.Ui")!.GetMethod("Theme")!.Invoke(null, [settings.Theme == "Dark"]);
        }
        Apply(config);
        using var hotkeys = new HotkeyService();
        if (!hotkeys.TryRegister(config, out string? error))
        {
            MessageBox.Show(error, "cap 开发预览快捷键被占用", MessageBoxButton.OK, MessageBoxImage.Information); return 1;
        }
        hotkeys.Activated += async command => await (Task)type.GetMethod("ExecuteAsync", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, [command])!;
        bool Save(AppSettings next)
        {
            next = next with { RunAtStartup = false, CloseToTray = false };
            if (!hotkeys.TryRegister(next, out string? message)) { MessageBox.Show(message, "cap 开发预览"); return false; }
            try { store.Save(next); config = next; Apply(next); return true; }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                var errors = new List<Exception> { ex };
                try
                {
                    if (!hotkeys.TryRegister(config, out string? rollbackError))
                        errors.Add(new InvalidOperationException("原快捷键恢复失败：" + rollbackError));
                }
                catch (Exception rollback) { errors.Add(new InvalidOperationException("原快捷键恢复失败：" + rollback.Message, rollback)); }
                Exception failure = errors.Count == 1 ? ex : new AggregateException("保存设置失败，且原快捷键未能恢复。请检查快捷键后重新保存。", errors);
                AppLog.Error(failure); MessageBox.Show(failure.Message, "cap 开发预览"); return false;
            }
        }
        type.GetMethod("CreateTray", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null);
        var tray = (System.Windows.Forms.NotifyIcon)type.GetField("tray", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(app)!;
        tray.Text = "cap 开发预览 · 保存代码后自动刷新";
        var window = (Window)Activator.CreateInstance(type.Assembly.GetType("Cap.App.SettingsWindow")!, [config, new Func<AppSettings, bool>(Save)])!;
        window.GetType().GetEvent("ThemeChangeRequested")!.AddEventHandler(window, new Action<string>(theme =>
        {
            var next = config with { Theme = theme };
            store.Save(next); config = next;
            type.GetField("settings", BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(app, next);
        }));
        hotkeys.CaptureGesture = gesture => (bool)window.GetType().GetMethod("CaptureHotkey")!.Invoke(window, [gesture])!;
        type.GetField("settingsWindow", BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(app, window);
        string previousCaption = window.Title.Replace("cap · ", "");
        string iteration = Environment.GetEnvironmentVariable("DOTNET_WATCH_ITERATION") ?? "1";
        string caption = $"开发预览 · 自动更新 · v{type.Assembly.GetName().Version!.ToString(3)} · #{iteration}";
        foreach (var text in Descendants(window)) if (text is TextBlock block && block.Text == previousCaption) { block.Text = caption; break; }
        window.Title = "cap · " + caption;
        foreach (string field in new[] { "startup", "closeToTray" })
        {
            var control = (CheckBox)window.GetType().GetField(field, BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(window)!;
            control.IsEnabled = false; control.ToolTip = "开发预览不修改系统自启动，关闭窗口结束本次预览。";
        }
        async Task Exit() => await (Task)type.GetMethod("ExitAsync", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null)!;
        window.GetType().GetEvent("ExitRequested")!.AddEventHandler(window, new Action(() => _ = Exit()));
        async Task Recover() => await (Task)type.GetMethod("RecoverAsync", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null)!;
        window.GetType().GetEvent("RecoveryRequested")!.AddEventHandler(window, new Action(() => _ = Recover()));
        async Task ConfigureRecording() => await (Task)type.GetMethod("ConfigureRecordingAsync", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null)!;
        window.GetType().GetEvent("AdvancedRecordingRequested")!.AddEventHandler(window, new Action(() => _ = ConfigureRecording()));
        window.Closing += (_, args) =>
        {
            if (!(bool)type.GetField("exiting", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(app)!) { args.Cancel = true; _ = Exit(); }
        };
        Console.CancelKeyPress += (_, args) => { args.Cancel = true; app.Dispatcher.BeginInvoke(new Action(app.Shutdown)); };
        Console.WriteLine($"DEV_PREVIEW_READY iteration={iteration} pid={Environment.ProcessId}");
        Console.WriteLine("截图 Ctrl+Shift+F10 · 录制 Ctrl+Shift+F11 · GIF Ctrl+Shift+F12；保存源码后自动重启预览。正式版设置与输出不受影响。");
        return app.Run(window);
    }
    private static IEnumerable<DependencyObject> Descendants(DependencyObject root)
    {
        foreach (object child in LogicalTreeHelper.GetChildren(root)) if (child is DependencyObject dependency)
        { yield return dependency; foreach (var nested in Descendants(dependency)) yield return nested; }
    }
}
