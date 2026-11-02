using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Shapes;
using Cap.Core;
using Cap.Capture;
using Cap.Recording;

namespace Cap.App;

internal sealed class RecordingControlWindow : Window
{
    private readonly TextBlock timer, savingStatus;
    private readonly Ellipse recordingDot;
    private readonly Button pause, stop;
    private bool paused, stopping;
    private bool closingAllowed;
    private RecordingRegionIndicator? regionIndicator;
    private PixelRect workArea;
    public event Action<bool>? PauseChanged;
    public event Action? StopRequested;
    public RecordingControlWindow(RecordingOptions options)
    {
        Ui.PrepareWindow(this);
        Title = "cap · 录制中"; Width = 350; Height = 56; WindowStyle = WindowStyle.None; AllowsTransparency = true;
        Background = Brushes.Transparent; Topmost = true; ShowInTaskbar = false; ResizeMode = ResizeMode.NoResize;
        var border = new Border { CornerRadius = new CornerRadius(28), BorderThickness = new Thickness(1), Padding = new Thickness(12, 8, 10, 8) };
        border.SetResourceReference(Border.BackgroundProperty, "WindowBrush");
        border.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        var row = new Grid(); border.Child = row;
        foreach (double width in new[] { 26d, 1d, 14d, 46d, 46d })
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = width == 1 ? new GridLength(1, GridUnitType.Star) : new GridLength(width) });
        var grip = new Border { Child = Ui.Icon("drag", 15), Cursor = Cursors.SizeAll, ToolTip = "拖动以移动控制条", Background = Brushes.Transparent };
        row.Children.Add(grip);
        var info = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center, Cursor = Cursors.SizeAll, Background = Brushes.Transparent };
        timer = new TextBlock { Text = "00:00", FontFamily = new FontFamily("Cascadia Mono,Consolas"), FontSize = 20, FontWeight = FontWeights.SemiBold };
        savingStatus = new TextBlock { Text = "正在保存…", FontSize = 11, Visibility = Visibility.Collapsed };
        savingStatus.SetResourceReference(TextBlock.ForegroundProperty, "MutedBrush");
        var progress = new StackPanel(); progress.Children.Add(timer); progress.Children.Add(savingStatus);
        recordingDot = new Ellipse { Width = 7, Height = 7, Fill = Ui.Brush("#F66B79"), VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(3, 0, 10, 0), ToolTip = "正在录制" };
        info.Children.Add(recordingDot); info.Children.Add(progress); Grid.SetColumn(info, 1); row.Children.Add(info);
        var separator = new Border { Width = 1, Height = 26, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        separator.SetResourceReference(Border.BackgroundProperty, "BorderBrush"); Grid.SetColumn(separator, 2); row.Children.Add(separator);
        pause = Ui.IconButton("pause", "暂停录制", TogglePause);
        pause.Width = 38; pause.Height = 38; pause.Margin = new Thickness(4, 0, 4, 0);
        Grid.SetColumn(pause, 3); row.Children.Add(pause);
        stop = Ui.IconButton("stop", "停止录制并保存", RequestStop);
        stop.Width = 38; stop.Height = 38; stop.Margin = new Thickness(4, 0, 4, 0);
        stop.Background = Ui.Brush("#E95368"); stop.Foreground = Brushes.White; stop.BorderBrush = Brushes.Transparent;
        stop.Resources["HoverBrush"] = Ui.Brush("#F46A7C");
        Grid.SetColumn(stop, 4); row.Children.Add(stop);
        grip.MouseLeftButtonDown += MoveControl;
        info.MouseLeftButtonDown += MoveControl;
        Content = border;
        SourceInitialized += (_, _) =>
        {
            nint handle = new WindowInteropHelper(this).Handle; CaptureService.ExcludeWindow(handle);
            var source = HwndSource.FromHwnd(handle); double scale = source?.CompositionTarget?.TransformToDevice.M11 ?? 1;
            int height = (int)Math.Ceiling(Height * scale), width = (int)Math.Ceiling(Width * scale);
            // Anchor inside the monitor WORK area: clamping to the full monitor let the bar sit on
            // the taskbar, where it flickered and clicks landed on the taskbar instead.
            PixelRect work = WorkArea(options.Bounds);
            (int x, int y) = RecordingBarPlacement.Place(options.Bounds, work, width, height);
            // HWND_TOPMOST keeps the bar above the taskbar; SWP_NOSIZE | SWP_NOACTIVATE.
            SetWindowPos(handle, -1, x, y, 0, 0, 0x15);
            workArea = work;
            LocationChanged += (_, _) => ClampOnScreen();
            if (!RecordingSession.UsesDirectRegionCapture(options)) return;
            regionIndicator ??= new RecordingRegionIndicator(options.Bounds);
            regionIndicator.Show();
        };
        Closing += (_, e) => { if (!closingAllowed) { e.Cancel = true; RequestStop(); } };
        Closed += (_, _) => { regionIndicator?.Dispose(); regionIndicator = null; };
    }
    /// <summary>Keeps the control bar inside the monitor work area after the user drags it.</summary>
    private void ClampOnScreen()
    {
        var source = HwndSource.FromHwnd(new WindowInteropHelper(this).Handle);
        double scale = source?.CompositionTarget?.TransformToDevice.M11 ?? 1;
        int width = (int)Math.Ceiling(ActualWidth * scale), height = (int)Math.Ceiling(ActualHeight * scale);
        if (width <= 0 || height <= 0 || workArea.Width <= 0) return;
        nint handle = new WindowInteropHelper(this).Handle;
        GetWindowRect(handle, out RECT current);
        int x = Math.Clamp(current.Left, workArea.X, Math.Max(workArea.X, workArea.Right - width));
        int y = Math.Clamp(current.Top, workArea.Y, Math.Max(workArea.Y, workArea.Bottom - height));
        if (x != current.Left || y != current.Top) SetWindowPos(handle, -1, x, y, 0, 0, 0x15);
    }

    /// <summary>Work area (taskbar excluded) of the monitor with the largest overlap.</summary>
    private static PixelRect WorkArea(PixelRect bounds)
    {
        RECT rect = UnsafeBounds(bounds);
        nint monitor = MonitorFromRect(ref rect, 2); // MONITOR_DEFAULTTONEAREST
        var info = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
        if (monitor == 0 || !GetMonitorInfo(monitor, ref info))
            return new PixelRect(bounds.X, bounds.Y, bounds.Width, bounds.Height);
        RECT work = info.rcWork;
        return new PixelRect(work.Left, work.Top, work.Right - work.Left, work.Bottom - work.Top);
    }

    private static RECT UnsafeBounds(PixelRect bounds) =>
        new() { Left = bounds.X, Top = bounds.Y, Right = bounds.Right, Bottom = bounds.Bottom };

    private void MoveControl(object sender, MouseButtonEventArgs e)
    {
        if (e.ButtonState == MouseButtonState.Pressed) DragMove();
    }
    private void RequestStop()
    {
        if (stopping || closingAllowed) return;
        BeginStopping();
        StopRequested?.Invoke();
    }
    public void BeginStopping()
    {
        if (stopping || closingAllowed) return;
        stopping = true; Title = "cap · 正在保存";
        pause.IsEnabled = false; stop.IsEnabled = false;
        recordingDot.ToolTip = "正在保存…"; recordingDot.Fill = Ui.Brush("#A3AABC");
        timer.FontSize = 16; savingStatus.Visibility = Visibility.Visible;
    }
    private void TogglePause()
    {
        if (stopping || closingAllowed) return;
        paused = !paused; PauseChanged?.Invoke(paused);
        regionIndicator?.SetPaused(paused);
        pause.Content = Ui.Icon(paused ? "play" : "pause", 18);
        pause.ToolTip = paused ? "继续录制" : "暂停录制";
        System.Windows.Automation.AutomationProperties.SetName(pause, paused ? "继续录制" : "暂停录制");
        recordingDot.ToolTip = paused ? "已暂停" : "正在录制";
        recordingDot.Fill = Ui.Brush(paused ? "#A3AABC" : "#F66B79");
    }
    public void Update(RecordingProgress progress)
    {
        if (closingAllowed || stopping) return;
        timer.Text = progress.Duration.ToString(progress.Duration.TotalHours >= 1 ? @"hh\:mm\:ss" : @"mm\:ss");
        timer.ToolTip = $"{progress.Backend} · {progress.FrameCount} 帧" + (progress.DroppedFrames > 0 ? $" · 跳过 {progress.DroppedFrames}" : "");
    }
    public void CloseAfterStop() { closingAllowed = true; Close(); }
    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint hwnd, nint insertAfter, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] private static extern nint MonitorFromRect(ref RECT rect, uint flags);
    [DllImport("user32.dll")] private static extern bool GetMonitorInfo(nint monitor, ref MONITORINFO info);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(nint hwnd, out RECT rect);
    [StructLayout(LayoutKind.Sequential)] private struct RECT { internal int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct MONITORINFO { internal int cbSize; internal RECT rcMonitor; internal RECT rcWork; internal int dwFlags; }
}
