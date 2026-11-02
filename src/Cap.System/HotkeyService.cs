using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Input;
using System.Windows.Interop;

namespace Cap.Platform;

public enum AppCommand { Screenshot = 1, Recording = 2, Gif = 3 }

public readonly record struct HotkeyGesture(uint Modifiers, uint VirtualKey)
{
    public static HotkeyGesture Parse(string text)
    {
        if (string.IsNullOrWhiteSpace(text)) throw new ArgumentException("快捷键不能为空。");
        uint modifiers = 0;
        uint key = 0;
        foreach (string raw in text.Split('+'))
        {
            string token = raw.Trim();
            switch (token.ToUpperInvariant())
            {
                case "CTRL": case "CONTROL": modifiers |= 2; break;
                case "ALT": modifiers |= 1; break;
                case "SHIFT": modifiers |= 4; break;
                case "WIN": case "WINDOWS": modifiers |= 8; break;
                default:
                    if (key != 0) throw new ArgumentException("快捷键只能有一个主按键。");
                    if (token.Length == 1 && char.IsAsciiDigit(token[0])) key = token[0];
                    else if (Enum.TryParse<Key>(token, true, out var parsed) && parsed != Key.None)
                        key = (uint)KeyInterop.VirtualKeyFromKey(parsed);
                    else throw new ArgumentException("无法识别快捷键：" + token);
                    break;
            }
        }
        if (modifiers == 0 || key == 0 || key is >= 0x10 and <= 0x12 || key is >= 0x5B and <= 0x5C || key is >= 0xA0 and <= 0xA5)
            throw new ArgumentException("快捷键需包含 Ctrl/Alt/Shift/Win 和一个普通按键。");
        return new(modifiers, key);
    }
}

public sealed class HotkeyService : IDisposable
{
    private readonly HwndSource source;
    private readonly Dictionary<AppCommand, HotkeyGesture> current = new();
    public event Action<AppCommand>? Activated;
    public Func<HotkeyGesture, bool>? CaptureGesture { get; set; }
    public HotkeyService()
    {
        source = new(new HwndSourceParameters("cap.hotkeys") { ParentWindow = new nint(-3), Width = 0, Height = 0, WindowStyle = 0 });
        source.AddHook(Hook);
    }

    public bool TryRegister(AppSettings settings, out string? error)
    {
        settings.Validate();
        var requested = new Dictionary<AppCommand, HotkeyGesture>
        {
            [AppCommand.Screenshot] = HotkeyGesture.Parse(settings.ScreenshotHotkey),
            [AppCommand.Recording] = HotkeyGesture.Parse(settings.RecordingHotkey),
            [AppCommand.Gif] = HotkeyGesture.Parse(settings.GifHotkey)
        };
        var previous = new Dictionary<AppCommand, HotkeyGesture>(current);
        UnregisterAll();
        foreach (var pair in requested)
        {
            if (!RegisterHotKey(source.Handle, (int)pair.Key, pair.Value.Modifiers | 0x4000, pair.Value.VirtualKey))
            {
                int nativeError = Marshal.GetLastWin32Error();
                UnregisterAll();
                foreach (var old in previous)
                    if (RegisterHotKey(source.Handle, (int)old.Key, old.Value.Modifiers | 0x4000, old.Value.VirtualKey)) current.Add(old.Key, old.Value);
                error = $"{pair.Key} 快捷键已被占用或不可用（Windows {nativeError}），请改用其他组合。";
                if (current.Count != previous.Count) error += " 原快捷键未能全部恢复，请重新保存设置。";
                return false;
            }
            current.Add(pair.Key, pair.Value);
        }
        error = null;
        return true;
    }
    private nint Hook(nint hwnd, int message, nint wParam, nint lParam, ref bool handled)
    {
        if (message == 0x0312 && Enum.IsDefined(typeof(AppCommand), wParam.ToInt32()))
        {
            handled = true;
            var command = (AppCommand)wParam.ToInt32();
            if (current.TryGetValue(command, out var gesture) && CaptureGesture?.Invoke(gesture) == true) return 0;
            Activated?.Invoke(command);
        }
        return 0;
    }
    private void UnregisterAll() { foreach (var pair in current) UnregisterHotKey(source.Handle, (int)pair.Key); current.Clear(); }
    public void Dispose() { UnregisterAll(); source.RemoveHook(Hook); source.Dispose(); }
    [DllImport("user32.dll", SetLastError = true)] private static extern bool RegisterHotKey(nint hwnd, int id, uint modifiers, uint key);
    [DllImport("user32.dll")] private static extern bool UnregisterHotKey(nint hwnd, int id);
}
