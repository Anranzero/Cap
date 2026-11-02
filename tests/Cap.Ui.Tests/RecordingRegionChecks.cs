using System;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Cap.Capture;
using Cap.Core;

internal static class RecordingRegionChecks
{
    internal static void Run(string outputDirectory)
    {
        var assembly = typeof(Cap.App.App).Assembly;
        Type indicatorType = assembly.GetType("Cap.App.RecordingRegionIndicator")
            ?? throw new Exception("Partial GIF recording needs a precise region indicator instead of the monitor capture border.");
        nint oldDpi = SetThreadDpiAwarenessContext(new nint(-4));
        if (oldDpi == 0) throw new Win32Exception(Marshal.GetLastWin32Error());
        Window? target = null;
        try
        {
            PixelRect monitor = CaptureService.GetMonitors()[0];
            int left = monitor.X + Math.Min(60, monitor.Width / 8), top = monitor.Y + Math.Min(60, monitor.Height / 8);
            target = new Window { Title = "cap owned region check", Width = 480, Height = 300, WindowStyle = WindowStyle.None,
                ResizeMode = ResizeMode.NoResize, ShowInTaskbar = false, ShowActivated = false, Topmost = true, Content = new TestPattern() };
            target.Show();
            nint targetHandle = new WindowInteropHelper(target).Handle;
            if (!SetWindowPos(targetHandle, new nint(-1), left, top, 480, 300, 0x10)) throw new Win32Exception(Marshal.GetLastWin32Error());
            Pump();
            PixelRect bounds = new(left + 30, top + 30, 300, 160);
            PixelFrame baseline = CaptureService.Capture(bounds);
            using (var indicator = (IDisposable)Activator.CreateInstance(indicatorType, [bounds])!)
            {
                indicatorType.GetMethod("Show")!.Invoke(indicator, null); Pump();
                var edges = Edges(indicatorType);
                CheckEdges(edges, bounds, targetHandle);
                if (!baseline.Pixels.SequenceEqual(CaptureService.Capture(bounds).Pixels))
                    throw new Exception("Region indicator leaked into the captured pixels.");
                indicatorType.GetMethod("SetPaused")!.Invoke(indicator, [true]);
                AssertColors(edges, "#A3AABC");
                indicatorType.GetMethod("SetPaused")!.Invoke(indicator, [false]);
                AssertColors(edges, "#087DFF");
                SaveRender(edges.First(edge => edge.ActualWidth > edge.ActualHeight), Path.Combine(outputDirectory, "recording-region-edge.png"));
            }
            Pump();
            if (Edges(indicatorType).Length != 0) throw new Exception("Disposed region indicator left a window open.");
            var negativeBounds = new PixelRect(-12420, -12280, 220, 140);
            using (var indicator = (IDisposable)Activator.CreateInstance(indicatorType, [negativeBounds])!)
            {
                indicatorType.GetMethod("Show")!.Invoke(indicator, null); Pump();
                CheckEdges(Edges(indicatorType), negativeBounds, 0);
            }
            Pump();

            Type controlsType = assembly.GetType("Cap.App.RecordingControlWindow")!;
            var options = new RecordingOptions { Bounds = bounds, Format = RecordingFormat.Gif, OutputPath = Path.Combine(outputDirectory, "region-check.gif") };
            var controls = (Window)Activator.CreateInstance(controlsType, [options])!;
            try
            {
                controls.Show(); Pump();
                CheckEdges(Edges(indicatorType), bounds, targetHandle);
                controlsType.GetMethod("TogglePause", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(controls, null);
                AssertColors(Edges(indicatorType), "#A3AABC");
                controlsType.GetMethod("TogglePause", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(controls, null);
                AssertColors(Edges(indicatorType), "#087DFF");
                controlsType.GetMethod("TogglePause", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(controls, null);
            }
            finally { controlsType.GetMethod("CloseAfterStop")!.Invoke(controls, null); Pump(); }
            if (Edges(indicatorType).Length != 0) throw new Exception("Closing recording controls left a region indicator open.");
            Console.WriteLine("PASS: GIF region indicator physical and negative-origin bounds, native transparent/noactivate styles, capture exclusion, pointer pass-through, pause/resume colors and paused control-close cleanup.");
        }
        finally
        {
            target?.Close();
            if (SetThreadDpiAwarenessContext(oldDpi) == 0) throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }
    private static Window[] Edges(Type indicatorType) => Application.Current.Windows.Cast<Window>()
        .Where(window => window.GetType().DeclaringType == indicatorType && window.IsVisible).ToArray();
    private static void CheckEdges(Window[] edges, PixelRect bounds, nint target)
    {
        if (edges.Length != 4) throw new Exception("A partial GIF must show exactly four narrow region edges.");
        var expected = new[] { new PixelRect(bounds.X, bounds.Y, bounds.Width, 2), new PixelRect(bounds.X, bounds.Bottom - 2, bounds.Width, 2),
            new PixelRect(bounds.X, bounds.Y, 2, bounds.Height), new PixelRect(bounds.Right - 2, bounds.Y, 2, bounds.Height) };
        var actual = edges.Select(window =>
        {
            nint handle = new WindowInteropHelper(window).Handle;
            if (!GetWindowRect(handle, out NativeRect rect)) throw new Win32Exception(Marshal.GetLastWin32Error());
            long styles = GetWindowLongPtrW(handle, -20).ToInt64();
            if ((styles & 0x080000A0L) != 0x080000A0L) throw new Exception("Region edge must be transparent, non-activating and a tool window.");
            if (!GetWindowDisplayAffinity(handle, out uint affinity) || affinity != 0x11) throw new Exception("Region edge was not excluded from capture.");
            return new PixelRect(rect.Left, rect.Top, rect.Right - rect.Left, rect.Bottom - rect.Top);
        }).ToArray();
        if (!actual.OrderBy(RectKey).SequenceEqual(expected.OrderBy(RectKey))) throw new Exception("Region edge positions did not match the physical selected bounds.");
        if (target != 0)
            foreach (var point in new[] { new NativePoint(bounds.X + 20, bounds.Y), new NativePoint(bounds.X + 20, bounds.Y + 20) })
                if (WindowFromPoint(point) != target) throw new Exception("Region indicator intercepted pointer hit testing.");
    }
    private static string RectKey(PixelRect rect) => $"{rect.X},{rect.Y},{rect.Width},{rect.Height}";
    private static void AssertColors(Window[] edges, string value)
    {
        if (edges.Length != 4 || edges.Any(edge => edge.Background is not SolidColorBrush brush || brush.Color != (Color)ColorConverter.ConvertFromString(value)))
            throw new Exception("Region edges did not reflect pause/resume state: " + value);
    }
    private static void Pump()
    {
        var frame = new DispatcherFrame();
        var timer = new DispatcherTimer(TimeSpan.FromMilliseconds(150), DispatcherPriority.Send,
            (_, _) => frame.Continue = false, Application.Current.Dispatcher);
        timer.Start();
        Application.Current.Dispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, new Action(() => frame.Continue = false));
        Dispatcher.PushFrame(frame);
        timer.Stop();
        int result = DwmFlush(); if (result != 0) Marshal.ThrowExceptionForHR(result);
    }
    private static void SaveRender(Window edge, string path)
    {
        var bitmap = new RenderTargetBitmap((int)Math.Ceiling(edge.ActualWidth), (int)Math.Ceiling(edge.ActualHeight), 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(edge); var png = new PngBitmapEncoder(); png.Frames.Add(BitmapFrame.Create(bitmap));
        using var stream = File.Create(path); png.Save(stream);
    }
    private sealed class TestPattern : FrameworkElement
    {
        protected override void OnRender(DrawingContext context)
        {
            context.DrawRectangle(Brushes.DarkOliveGreen, null, new Rect(RenderSize));
            for (int y = 0; y < RenderSize.Height; y += 16)
                for (int x = 0; x < RenderSize.Width; x += 16)
                    if (((x + y) / 16 & 1) == 0) context.DrawRectangle(Brushes.Goldenrod, null, new Rect(x, y, 8, 8));
        }
    }
    [StructLayout(LayoutKind.Sequential)] private readonly record struct NativePoint(int X, int Y);
    [StructLayout(LayoutKind.Sequential)] private struct NativeRect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", SetLastError = true)] private static extern nint SetThreadDpiAwarenessContext(nint context);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetWindowRect(nint handle, out NativeRect rect);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetWindowDisplayAffinity(nint handle, out uint affinity);
    [DllImport("user32.dll")] private static extern nint GetWindowLongPtrW(nint handle, int index);
    [DllImport("user32.dll")] private static extern nint WindowFromPoint(NativePoint point);
    [DllImport("dwmapi.dll")] private static extern int DwmFlush();
}
