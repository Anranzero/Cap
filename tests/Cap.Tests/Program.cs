using Cap.Core;
using Cap.Platform;
using System.IO;

int failures = 0;
void Check(string name, Action test)
{
    try { test(); Console.WriteLine("PASS " + name); }
    catch (Exception ex) { failures++; Console.WriteLine("FAIL " + name + ": " + ex.Message); }
}
void Assert(bool condition, string message) { if (!condition) throw new Exception(message); }
Check("Negative-origin region intersection", () => Assert(new PixelRect(-1920, -200, 1920, 1080).Intersect(new(-50, 0, 100, 100)) == new PixelRect(-50, 0, 50, 100), "Physical bounds mismatch"));
Check("Exclusive right/bottom edges", () => Assert(!new PixelRect(-10, -10, 20, 20).Contains(10, 0) && new PixelRect(-10, -10, 20, 20).Contains(-10, -10), "Invalid hit test"));
Check("Invalid frame rejects undersized pixels", () => { try { new PixelFrame(10, 10, new byte[4]).Validate(); } catch (ArgumentException) { return; } throw new Exception("Accepted undersized frame"); });
Check("Recording bar stays clear of the taskbar", () =>
{
    // 2560x1440 monitor, 48px taskbar: work area is 0..1392.
    var work = new PixelRect(0, 0, 2560, 1392);
    var fullScreen = new PixelRect(0, 0, 2560, 1440);
    (int x, int y) = RecordingBarPlacement.Place(fullScreen, work, 525, 84);
    Assert(y >= 0 && y + 84 <= 1392, $"Full-screen bar overlapped the taskbar: y={y}");
    Assert(y == 1392 - 84 - 8, $"Full-screen bar should hug the bottom of the work area: y={y}");
    Assert(x == (2560 - 525) / 2, $"Full-screen bar should be centred: x={x}");
    // Region ending just above the taskbar: the bar must move above the region.
    var lowRegion = new PixelRect(0, 400, 1200, 960);
    (int lx, int ly) = RecordingBarPlacement.Place(lowRegion, work, 525, 84);
    Assert(ly + 84 <= 1392, $"Bar below a low region overlapped the taskbar: y={ly}");
    Assert(ly + 84 <= 400 || ly >= 1360 - 84 - 8, $"Bar was not placed sensibly: y={ly}");
    // Region with room below keeps the classic below-the-region position.
    var midRegion = new PixelRect(100, 200, 800, 600);
    (int mx, int my) = RecordingBarPlacement.Place(midRegion, work, 525, 84);
    Assert(my == 200 + 600 + 8, $"Bar did not sit below a mid-screen region: y={my}");
    Assert(mx == 100, $"Bar x drifted: x={mx}");
});
Check("Recording bar clamps inside a negative-origin monitor", () =>
{
    var work = new PixelRect(-1920, 0, 1920, 1040);
    var bounds = new PixelRect(-1920, 0, 1920, 1080);
    (int x, int y) = RecordingBarPlacement.Place(bounds, work, 525, 84);
    Assert(x >= -1920 && x + 525 <= 0, $"Bar left the monitor: x={x}");
    Assert(y >= 0 && y + 84 <= 1040, $"Bar left the work area: y={y}");
});Check("Settings roundtrip and last-good backup", () =>
{
    string dir = Path.Combine(Path.GetTempPath(), "cap-settings-tests-" + Guid.NewGuid().ToString("N"));
    try
    {
        var store = new SettingsStore(dir);
        var settings = new AppSettings { SaveDirectory = dir, Theme = "Light", RelativeCoordinates = true, CoordinateColor = "#00FF99" };
        store.Save(settings); Assert(store.Load() == settings, "Roundtrip lost settings");
        store.Save(settings with { Theme = "Dark" }); Assert(File.Exists(store.FilePath + ".bak"), "Missing last-good backup");
        Assert(File.ReadAllText(store.FilePath + ".bak").Contains("Light"), "Backup is not previous configuration");
    }
    finally { if (Directory.Exists(dir)) Directory.Delete(dir, true); }
});
Check("Corrupt settings preserve original and warn", () =>
{
    string dir = Path.Combine(Path.GetTempPath(), "cap-corrupt-tests-" + Guid.NewGuid().ToString("N"));
    try
    {
        Directory.CreateDirectory(dir); var store = new SettingsStore(dir); File.WriteAllText(store.FilePath, "{invalid}");
        _ = store.Load(); Assert(store.LoadWarning != null && File.ReadAllText(store.FilePath) == "{invalid}", "Corrupt configuration was silently erased");
    }
    finally { if (Directory.Exists(dir)) Directory.Delete(dir, true); }
});
Check("Hotkey normalization", () => Assert(HotkeyGesture.Parse("Ctrl + Alt + a") == HotkeyGesture.Parse("alt+CONTROL+A"), "Equivalent gestures differ"));
Check("Modifier-only shortcut is rejected", () => { try { HotkeyGesture.Parse("Ctrl+Shift"); } catch (ArgumentException) { return; } throw new Exception("Accepted modifier-only shortcut"); });
Check("Duplicate normalized hotkeys rejected", () => { try { new AppSettings { GifHotkey = "ALT+CTRL+a" }.Validate(); } catch (ArgumentException) { return; } throw new Exception("Accepted duplicate shortcut"); });
Check("Invalid frame rate rejected", () => { try { new AppSettings { FramesPerSecond = 120 }.Validate(); } catch (ArgumentException) { return; } throw new Exception("Accepted unsupported MP4 FPS"); });
return failures == 0 ? 0 : 1;
