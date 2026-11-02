using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;
using Cap.Platform;

internal static class InteractiveScreenshotSmoke
{
    internal static int Run(Cap.App.App app, string output)
    {
        var config = new AppSettings { SaveDirectory = output, Theme = "Light", ScreenshotHotkey = "Ctrl+Shift+F10", RecordingHotkey = "Ctrl+Shift+F11", GifHotkey = "Ctrl+Shift+F12" };
        Type type = typeof(Cap.App.App);
        type.GetField("settings", BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(app, config);
        type.Assembly.GetType("Cap.App.Ui")!.GetMethod("Theme")!.Invoke(null, [false]);
        var previousClipboard = Clipboard.GetDataObject();
        using var hotkeys = new HotkeyService();
        if (!hotkeys.TryRegister(config, out string? error)) throw new InvalidOperationException(error);
        hotkeys.Activated += async command => await (Task)type.GetMethod("ExecuteAsync", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, [command])!;
        var observed = new HashSet<Window>();
        // Test-only: allow the native inspection tool to see this harness's capture overlay.
        // The production application continues to exclude every overlay from capture.
        var inspection = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(100) };
        inspection.Tick += (_, _) =>
        {
            foreach (Window window in app.Windows)
                if (window.IsVisible && observed.Add(window))
                {
                    window.ShowInTaskbar = true;
                    if (window.GetType().FullName == "Cap.Annotation.ScreenshotWindow" && !SetWindowDisplayAffinity(new WindowInteropHelper(window).Handle, 0))
                        throw new Win32Exception(Marshal.GetLastWin32Error());
                }
        };
        try
        {
            inspection.Start(); type.GetMethod("ShowSettings", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null);
            Console.WriteLine("Interactive smoke ready: Ctrl+Shift+F10 screenshot / F11 recording / F12 GIF. Uses actual App.ExecuteAsync and temporary output. Exit using Settings.");
            return app.Run();
        }
        finally { inspection.Stop(); if (previousClipboard != null) Clipboard.SetDataObject(previousClipboard, true); else Clipboard.Clear(); }
    }
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowDisplayAffinity(nint window, uint affinity);
}
