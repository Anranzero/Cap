using System.Diagnostics;
using System.Runtime.InteropServices;
using Cap.Capture;
using Cap.Core;

namespace Cap.Capture.Tests;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        try
        {
            RunPureChecks();
            if (args.Contains("--native")) RunDesktopChecks();
            Console.WriteLine("Capture checks passed.");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error);
            return 1;
        }
    }

    private static void RunPureChecks()
    {
        Expect<ArgumentOutOfRangeException>(() => CaptureService.Capture(new(0, 0, 0, 10)));
        Expect<ArgumentOutOfRangeException>(() => CaptureService.CreateSource(new(new(0, 0, 10, -1))));
        Expect<OverflowException>(() => CaptureService.CreateSource(new(new(int.MaxValue, 0, 2, 2))));
        Expect<ArgumentNullException>(() => CaptureService.ToBitmapSource(null!));
        Expect<ArgumentException>(() => CaptureService.ToBitmapSource(new(1, 1, [])));
        Expect<ArgumentException>(() => CaptureService.ExcludeWindow(0));
        var frame = new PixelFrame(2, 1, [1, 2, 3, 255, 4, 5, 6, 255]);
        var bitmap = CaptureService.ToBitmapSource(frame);
        Check(bitmap.IsFrozen && bitmap.PixelWidth == 2 && bitmap.PixelHeight == 1, "Frozen bitmap conversion");
        byte[] copy = new byte[8];
        bitmap.CopyPixels(copy, 8, 0);
        Check(copy.SequenceEqual(frame.Pixels), "Bitmap BGRA channel order");
    }

    private static void RunDesktopChecks()
    {
        Trace.Listeners.Add(new ConsoleTraceListener());
        PixelRect desktop = CaptureService.GetVirtualBounds();
        var monitors = CaptureService.GetMonitors();
        Check(!desktop.IsEmpty && monitors.Count > 0, "Monitor enumeration");
        Check(monitors.All(rect => rect.Intersect(desktop) == rect), "Physical monitor coordinates");
        var bounds = new PixelRect(monitors[0].X, monitors[0].Y, Math.Min(96, monitors[0].Width), Math.Min(64, monitors[0].Height));
        PixelFrame frame = CaptureService.Capture(bounds);
        ValidateFrame(frame, bounds);
        using (var gdi = CaptureService.CreateSource(new(bounds, PreferGraphicsCapture: false)))
        {
            Check(gdi.BackendName == "GDI", "Explicit GDI source");
            ValidateFrame(gdi.Capture(), bounds);
            gdi.Dispose();
            Expect<ObjectDisposedException>(() => gdi.Capture());
        }
        PixelRect outside = new(desktop.X - 12, desktop.Y - 10, 8, 6);
        PixelFrame padding = CaptureService.Capture(outside);
        ValidateFrame(padding, outside);
        Check(padding.Pixels.Where((_, index) => index % 4 != 3).All(value => value == 0), "Out-of-desktop black padding");
        using (var automatic = CaptureService.CreateSource(new(bounds)))
        {
            Console.WriteLine($"Automatic region backend before capture: {automatic.BackendName}");
            ValidateFrame(automatic.Capture(), bounds);
            ValidateFrame(automatic.Capture(), bounds);
            Console.WriteLine($"Automatic region backend after capture: {automatic.BackendName}");
            Check(automatic.Bounds == bounds, "Stable source bounds");
        }
        var windows = CaptureService.GetWindows();
        Console.WriteLine($"Desktop {desktop}; monitors={monitors.Count}; visible titled windows={windows.Count}");
        Console.WriteLine($"Window hit-test at sample point: {CaptureService.GetWindowAt(bounds.X + 10, bounds.Y + 10)?.Handle ?? 0}");
        if (windows.Count > 0)
        {
            var target = windows[0];
            var requested = new PixelRect(target.Bounds.X, target.Bounds.Y, Math.Min(96, target.Bounds.Width), Math.Min(64, target.Bounds.Height));
            using var window = CaptureService.CreateSource(new(requested, target.Handle));
            ValidateFrame(window.Capture(), requested);
            Console.WriteLine($"Window backend: {window.BackendName}");
        }
        using var process = Process.GetCurrentProcess();
        int start = GetGuiResources(process.Handle, 0);
        for (int index = 0; index < 25; index++) ValidateFrame(CaptureService.Capture(bounds), bounds);
        int finish = GetGuiResources(process.Handle, 0);
        Check(finish <= start + 2, $"GDI resource stability ({start} -> {finish})");
        RunWindowLifecycleChecks(bounds);
    }

    private static void RunWindowLifecycleChecks(PixelRect sample)
    {
        // An off-desktop native window gives WGC a real drawable HWND without visible test UI.
        PixelRect desktop = CaptureService.GetVirtualBounds();
        nint target = CreateWindowExW(0, "STATIC", "Cap capture lifecycle check", 0x90000000,
            desktop.Right + 100, desktop.Bottom + 100, 100, 80, 0, 0, 0, 0);
        Check(target != 0, "Create lifecycle test window");
        try
        {
            using var gdi = CaptureService.CreateSource(new(sample, target, false));
            ValidateFrame(gdi.Capture(), sample);
            Check(gdi.Bounds == sample, "GDI retains original desktop rectangle for window requests");
            using var graphics = CaptureService.CreateSource(new(sample, target));
            Console.WriteLine($"Window lifecycle backend: {graphics.BackendName}");
            if (graphics.BackendName == "Windows Graphics Capture")
            {
                ValidateFrame(graphics.Capture(), sample);
                if (graphics.BackendName == "Windows Graphics Capture")
                {
                    var pacing = Stopwatch.StartNew();
                    for (int index = 0; index < 60; index++) ValidateFrame(graphics.Capture(), sample);
                    pacing.Stop();
                    Console.WriteLine($"Static window WGC: 60 capture calls in {pacing.Elapsed.TotalMilliseconds:F1} ms");
                    Check(pacing.Elapsed < TimeSpan.FromSeconds(1), "Cached WGC frames must not impose a 100 ms wait or cap overlay updates below 60 FPS");
                }
                PixelRect before = graphics.Bounds;
                Check(SetWindowPos(target, 0, before.X + 30, before.Y + 20, 0, 0, 0x15), "Move hidden test window");
                PixelRect after = graphics.Bounds;
                Check(after.X == before.X + 30 && after.Y == before.Y + 20, "WGC live physical window origin");
                Check(after.Width == sample.Width && after.Height == sample.Height, "WGC fixed output dimensions after movement");
            }
            ShowWindow(target, 0);
            using var hiddenFallback = CaptureService.CreateSource(new(sample, target));
            Console.WriteLine($"Hidden window fallback backend: {hiddenFallback.BackendName}");
            Check(DestroyWindow(target), "Destroy lifecycle test window");
            target = 0;
            Expect<InvalidOperationException>(() => gdi.Capture());
            Expect<InvalidOperationException>(() => graphics.Capture());
            Expect<InvalidOperationException>(() => hiddenFallback.Capture());
        }
        finally
        {
            if (target != 0) DestroyWindow(target);
        }
    }

    private static void ValidateFrame(PixelFrame frame, PixelRect bounds)
    {
        frame.Validate();
        Check(frame.Width == bounds.Width && frame.Height == bounds.Height, "Requested frame dimensions");
        for (int index = 3; index < frame.Pixels.Length; index += 4)
            if (frame.Pixels[index] != 255) throw new InvalidOperationException("Frame alpha must be opaque.");
    }

    private static void Check(bool condition, string name)
    {
        if (!condition) throw new InvalidOperationException(name);
    }

    private static void Expect<T>(Action action) where T : Exception
    {
        try { action(); }
        catch (T) { return; }
        throw new InvalidOperationException($"Expected {typeof(T).Name}.");
    }

    [DllImport("user32.dll")] private static extern int GetGuiResources(nint process, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] private static extern nint CreateWindowExW(uint extendedStyle,
        string className, string title, uint style, int x, int y, int width, int height, nint parent, nint menu, nint instance, nint parameter);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SetWindowPos(nint window, nint insertAfter, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool DestroyWindow(nint window);
    [DllImport("user32.dll")] private static extern bool ShowWindow(nint window, int command);
}
