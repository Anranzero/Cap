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
            PixelRect monitor = CaptureService.GetMonitors().OrderByDescending(m =>
            {
                PixelRect overlap = m.Intersect(options.Bounds); return (long)overlap.Width * overlap.Height;
            }).First();
            var source = HwndSource.FromHwnd(handle); double scale = source?.CompositionTarget?.TransformToDevice.M11 ?? 1;
            int height = (int)Math.Ceiling(Height * scale), width = (int)Math.Ceiling(Width * scale);
            int x = Math.Clamp(options.Bounds.X, monitor.X, Math.Max(monitor.X, monitor.Right - width));
            int y = options.Bounds.Bottom + 8;
            if (y + height > monitor.Bottom || y < monitor.Y) y = options.Bounds.Y - height - 8;
            y = Math.Clamp(y, monitor.Y + 8, Math.Max(monitor.Y + 8, monitor.Bottom - height - 8));
            SetWindowPos(handle, 0, x, y, 0, 0, 0x15);
            if (!RecordingSession.UsesDirectRegionCapture(options)) return;
            regionIndicator ??= new RecordingRegionIndicator(options.Bounds);
            regionIndicator.Show();
        };
        Closing += (_, e) => { if (!closingAllowed) { e.Cancel = true; RequestStop(); } };
        Closed += (_, _) => { regionIndicator?.Dispose(); regionIndicator = null; };
    }
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
}
