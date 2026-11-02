using System;
using System.IO;

namespace Cap.Platform;

public sealed record AppSettings
{
    public string SaveDirectory { get; init; } = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "cap");
    public string ScreenshotHotkey { get; init; } = "Ctrl+Alt+A";
    public string RecordingHotkey { get; init; } = "Ctrl+Alt+V";
    public string GifHotkey { get; init; } = "Ctrl+Alt+G";
    public string Theme { get; init; } = "Light";
    public bool RunAtStartup { get; init; }
    public bool CloseToTray { get; init; } = true;
    public int FramesPerSecond { get; init; } = 60;
    public int GifFramesPerSecond { get; init; } = 15;
    public int GifMaxWidth { get; init; } = 960;
    public bool ShowCursor { get; init; } = true;
    public bool HighlightClicks { get; init; } = true;
    public bool ShowCoordinates { get; init; } = true;
    public bool RelativeCoordinates { get; init; }
    public int CoordinateFontSize { get; init; } = 18;
    public string CoordinateColor { get; init; } = "#FFFFFF";

    public void Validate()
    {
        if (string.IsNullOrWhiteSpace(SaveDirectory) || !Path.IsPathFullyQualified(SaveDirectory))
            throw new ArgumentException("保存目录必须是完整路径。");
        if (Theme is not ("Dark" or "Light")) throw new ArgumentException("主题无效。");
        if (FramesPerSecond is not (15 or 24 or 30 or 60)) throw new ArgumentException("MP4 帧率请选择 15、24、30 或 60。");
        if (GifFramesPerSecond is < 1 or > 60 || GifMaxWidth is < 64 or > 7680)
            throw new ArgumentException("GIF 帧率范围为 1–60，宽度范围为 64–7680。");
        if (CoordinateFontSize is < 10 or > 72) throw new ArgumentException("坐标字号范围为 10–72。");
        if (CoordinateColor == null || !global::System.Text.RegularExpressions.Regex.IsMatch(CoordinateColor, "^#[0-9A-Fa-f]{6}$"))
            throw new ArgumentException("坐标颜色请填写 #RRGGBB。");
        var a = HotkeyGesture.Parse(ScreenshotHotkey);
        var b = HotkeyGesture.Parse(RecordingHotkey);
        var c = HotkeyGesture.Parse(GifHotkey);
        if (a == b || a == c || b == c) throw new ArgumentException("三个快捷键不能重复。");
    }
}
