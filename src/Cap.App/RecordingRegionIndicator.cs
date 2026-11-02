using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using Cap.Capture;
using Cap.Core;

namespace Cap.App;

/// <summary>Four small, input-transparent windows outline the physical recording area.</summary>
internal sealed class RecordingRegionIndicator : IDisposable
{
    private static readonly SolidColorBrush RecordingBrush = FrozenBrush(8, 125, 255);
    private static readonly SolidColorBrush PausedBrush = FrozenBrush(163, 170, 188);
    private readonly PixelRect bounds;
    private readonly List<RegionEdgeWindow> edges = [];
    private bool shown, paused, disposed;

    public RecordingRegionIndicator(PixelRect bounds)
    {
        if (bounds.IsEmpty) throw new ArgumentOutOfRangeException(nameof(bounds));
        _ = bounds.Right; _ = bounds.Bottom;
        this.bounds = bounds;
    }
    public void Show()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (shown) return;
        int thicknessX = Math.Min(2, bounds.Width), thicknessY = Math.Min(2, bounds.Height);
        var strips = new[]
        {
            new PixelRect(bounds.X, bounds.Y, bounds.Width, thicknessY),
            new PixelRect(bounds.X, bounds.Bottom - thicknessY, bounds.Width, thicknessY),
            new PixelRect(bounds.X, bounds.Y, thicknessX, bounds.Height),
            new PixelRect(bounds.Right - thicknessX, bounds.Y, thicknessX, bounds.Height)
        };
        try
        {
            foreach (PixelRect strip in strips)
            {
                var edge = new RegionEdgeWindow(strip, paused ? PausedBrush : RecordingBrush);
                edges.Add(edge);
                edge.Show();
            }
            shown = true;
        }
        catch (Exception creationError)
        {
            try { Dispose(); }
            catch (Exception cleanupError) { throw new AggregateException("Cannot create or clean up the recording region indicator.", creationError, cleanupError); }
            throw;
        }
    }
    public void SetPaused(bool value)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        paused = value;
        foreach (var edge in edges) edge.Background = value ? PausedBrush : RecordingBrush;
    }
    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        List<Exception>? errors = null;
        foreach (var edge in edges)
        {
            try { edge.Close(); }
            catch (Exception error) { (errors ??= []).Add(error); }
        }
        edges.Clear();
        if (errors != null) throw new AggregateException("Cannot close every recording region edge.", errors);
    }
    private static SolidColorBrush FrozenBrush(byte red, byte green, byte blue)
    {
        var brush = new SolidColorBrush(Color.FromRgb(red, green, blue)); brush.Freeze(); return brush;
    }
    private sealed class RegionEdgeWindow : Window
    {
        private const int ExtendedStyle = -20;
        private const long TransparentNoActivateToolWindow = 0x080000A0L;
        private readonly PixelRect physicalBounds;
        private HwndSource? source;
        public RegionEdgeWindow(PixelRect bounds, Brush brush)
        {
            physicalBounds = bounds;
            Title = "cap · 录制范围";
            WindowStyle = WindowStyle.None; ResizeMode = ResizeMode.NoResize; AllowsTransparency = true;
            Background = brush; ShowActivated = false; ShowInTaskbar = false; Topmost = true;
            Focusable = false; IsHitTestVisible = false; WindowStartupLocation = WindowStartupLocation.Manual;
            Left = bounds.X; Top = bounds.Y; Width = bounds.Width; Height = bounds.Height;
            SourceInitialized += (_, _) =>
            {
                nint handle = new WindowInteropHelper(this).Handle;
                nint previous = SetWindowLongPtrW(handle, ExtendedStyle, new nint(GetWindowLongPtrW(handle, ExtendedStyle).ToInt64() | TransparentNoActivateToolWindow));
                if (previous == 0 && Marshal.GetLastWin32Error() != 0) throw new Win32Exception(Marshal.GetLastWin32Error());
                CaptureService.ExcludeWindow(handle);
                source = HwndSource.FromHwnd(handle); source?.AddHook(WindowMessage);
                Place();
            };
            Loaded += (_, _) => Place();
            Closed += (_, _) => source?.RemoveHook(WindowMessage);
        }
        private nint WindowMessage(nint hwnd, int message, nint wParam, nint lParam, ref bool handled)
        {
            if (message == 0x0084) { handled = true; return new nint(-1); } // HTTRANSPARENT
            if (message == 0x0021) { handled = true; return new nint(3); } // MA_NOACTIVATE
            if (message == 0x02E0)
                Dispatcher.BeginInvoke(DispatcherPriority.Loaded, new Action(() => { if (IsVisible) Place(); }));
            return 0;
        }
        private void Place()
        {
            if (!SetWindowPos(new WindowInteropHelper(this).Handle, new nint(-1), physicalBounds.X, physicalBounds.Y,
                physicalBounds.Width, physicalBounds.Height, 0x0010))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot position the recording region edge.");
        }
        [DllImport("user32.dll", SetLastError = true)] private static extern nint GetWindowLongPtrW(nint handle, int index);
        [DllImport("user32.dll", SetLastError = true)] private static extern nint SetWindowLongPtrW(nint handle, int index, nint value);
        [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);
    }
}
