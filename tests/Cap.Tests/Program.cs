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
Check("Settings roundtrip and last-good backup", () =>
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
