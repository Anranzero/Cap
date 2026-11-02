using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Cap.Capture;
using Cap.Core;
using Cap.Visuals;

namespace Cap.Annotation;

public static class ScrollingCaptureWorkflow
{
    public static Task<BitmapSource?> RunAsync(PixelRect region, string saveDirectory, bool darkTheme = false, Point? controlPosition = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(saveDirectory);
        if (region.Width < 64 || region.Height < 80)
            throw new ArgumentException("滚动截图区域至少需要 64 × 80 像素，请重新框选内容区域。", nameof(region));
        if (Application.Current is null) throw new InvalidOperationException("滚动截图需要正在运行的 WPF Application。");
        Task<BitmapSource?> Run()
        {
            var window = new ScrollingCaptureWindow(region, darkTheme, controlPosition);
            try { window.Show(); }
            catch (Exception error)
            {
                try { window.Close(); }
                catch (Exception cleanup) { throw new AggregateException("无法打开或关闭滚动截图浮条。", error, cleanup); }
                throw;
            }
            return window.Completion;
        }
        return Application.Current.Dispatcher.CheckAccess()
            ? Run() : Application.Current.Dispatcher.InvokeAsync(Run).Task.Unwrap();
    }
}

internal sealed class ScrollingCaptureWindow : Window
{
    private const int CancelHotkeyId = 0x5343;
    private readonly PixelRect region;
    private readonly ScrollingCaptureStitcher stitcher;
    private readonly DispatcherTimer timer;
    private readonly TextBlock status;
    private readonly Button automaticButton;
    private readonly Button finishButton;
    private readonly Brush foreground;
    private readonly ScrollingCaptureOutline outline;
    private readonly ScrollingCaptureInput input;
    private readonly bool hosted;
    private readonly TaskCompletionSource<BitmapSource?> completion = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private PixelFrame? candidate;
    private string? initializationError;
    private bool stopped;
    private bool excluded;
    private bool autoScrolling;
    private bool finishing;
    private bool capturePaused;
    private bool limitReached;
    private bool finishRetained;
    private bool awaitingRetainedViewport;
    private bool cancelHotkeyRegistered;
    private HwndSource? source;
    private int unchangedScrolls;
    public BitmapSource? Result { get; private set; }
    internal Task<BitmapSource?> Completion => completion.Task;

    public ScrollingCaptureWindow(PixelRect region, bool darkTheme, Point? controlPosition = null, Window? owner = null)
    {
        this.region = region;
        hosted = owner is not null;
        if (owner is not null) Owner = owner;
        // Take the first viewport before creating our own floating control window.
        stitcher = new ScrollingCaptureStitcher(CaptureService.Capture(region));
        outline = new ScrollingCaptureOutline(region);
        input = new ScrollingCaptureInput(region);
        Title = "Cap 滚动截图";
        Width = 408; Height = 48;
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.NoResize;
        Topmost = true;
        ShowInTaskbar = false;
        ShowActivated = false;
        WindowStartupLocation = WindowStartupLocation.Manual;
        foreground = Brush(darkTheme ? "#F5F3F5" : "#262627");
        AllowsTransparency = true;
        Background = Brushes.Transparent;
        Foreground = foreground;
        FontFamily = new FontFamily("Microsoft YaHei UI");
        FontSize = 12;
        var row = new DockPanel { Margin = new Thickness(7, 5, 7, 5), LastChildFill = true };
        var grip = new Border { Width = 24, Background = Brushes.Transparent, Cursor = Cursors.SizeAll, ToolTip = "拖动位置", Child = VectorIcon.Create("drag", 15, Brush("#8C8992")) };
        grip.MouseLeftButtonDown += (_, e) =>
        {
            if (e.LeftButton != MouseButtonState.Pressed) return;
            e.Handled = true;
            DragMove();
            if (!stopped) ScrollingCapturePlacement.Constrain(this);
        };
        DockPanel.SetDock(grip, Dock.Left); row.Children.Add(grip);
        var actions = new StackPanel { Orientation = Orientation.Horizontal };
        automaticButton = Action("chevronDown", "自动向下滚动", ToggleAutomatic, darkTheme);
        finishButton = Action("check", "完成", () => _ = FinishAsync(), darkTheme);
        finishButton.Content = VectorIcon.Create("check", 20, Brush("#28C76F"));
        actions.Children.Add(automaticButton);
        actions.Children.Add(new Border { Width = 1, Height = 18, Margin = new Thickness(5, 0, 5, 0), Background = Brush(darkTheme ? "#494749" : "#E4E0E4"), VerticalAlignment = VerticalAlignment.Center });
        actions.Children.Add(Action("close", "取消", Close, darkTheme));
        actions.Children.Add(finishButton);
        DockPanel.SetDock(actions, Dock.Right); row.Children.Add(actions);
        status = new TextBlock { Text = "滚动鼠标滚轮截取", FontSize = 13, Margin = new Thickness(5, 0, 8, 0), VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis, ToolTip = Dimensions() };
        row.Children.Add(status);
        Content = new Border { Background = AnnotationChrome.Surface(darkTheme), BorderBrush = Brush(darkTheme ? "#494749" : "#E4E0E4"), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(14), Child = row };

        timer = new DispatcherTimer(DispatcherPriority.Background) { Interval = TimeSpan.FromMilliseconds(400) };
        timer.Tick += OnTick;
        SourceInitialized += (_, _) =>
        {
            try
            {
                nint handle = new WindowInteropHelper(this).Handle;
                CaptureService.ExcludeWindow(handle);
                excluded = true;
                source = HwndSource.FromHwnd(handle);
                source?.AddHook(WindowMessage);
                cancelHotkeyRegistered = RegisterHotKey(handle, CancelHotkeyId, 0x4000, 0x1B);
                if (!cancelHotkeyRegistered)
                    System.Diagnostics.Trace.TraceWarning("Cannot register scrolling capture Escape: {0}", Marshal.GetLastWin32Error());
            }
            catch (Win32Exception error) { initializationError = error.Message; }
        };
        Loaded += (_, _) =>
        {
            ScrollingCapturePlacement.Place(this, region, controlPosition);
            if (initializationError is not null)
            {
                automaticButton.IsEnabled = false;
                ShowStatus("无法开始，请取消后重试", initializationError);
                return;
            }
            try
            {
                outline.Show(); timer.Start();
                // Focusing the page keeps the real wheel working even when hover scrolling is turned off.
                if (!input.ActivateTarget() && !hosted) ShowStatus("点击选区后滚动鼠标滚轮");
            }
            catch (Exception error) when (IsCaptureError(error)) { CaptureFailed(error); }
        };
        DpiChanged += (_, _) => Dispatcher.BeginInvoke(DispatcherPriority.Loaded, new Action(() => { if (!stopped && IsVisible) ScrollingCapturePlacement.Constrain(this); }));
        Closing += (_, _) =>
        {
            if (!cancelHotkeyRegistered) return;
            if (!UnregisterHotKey(new WindowInteropHelper(this).Handle, CancelHotkeyId))
                System.Diagnostics.Trace.TraceWarning("Cannot unregister scrolling capture Escape: {0}", Marshal.GetLastWin32Error());
            cancelHotkeyRegistered = false;
        };
        Closed += (_, _) =>
        {
            stopped = true; timer.Stop(); timer.Tick -= OnTick; candidate = null;
            source?.RemoveHook(WindowMessage);
            try { outline.Dispose(); }
            finally { completion.TrySetResult(Result); }
        };
        PreviewKeyDown += (_, e) =>
        {
            if (e.Key == Key.Escape) { Close(); e.Handled = true; }
            else if (e.Key == Key.Enter) { _ = FinishAsync(); e.Handled = true; }
            else if (e.Key == Key.Z && Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) { Undo(); e.Handled = true; }
        };
    }

    private nint WindowMessage(nint hwnd, int message, nint wParam, nint lParam, ref bool handled)
    {
        if (message == 0x0312 && wParam.ToInt32() == CancelHotkeyId)
        {
            handled = true;
            Dispatcher.BeginInvoke(new Action(() => { if (!stopped) Close(); }));
        }
        return 0;
    }

    private void OnTick(object? sender, EventArgs e)
    {
        if (stopped || finishing || !excluded || limitReached) return;
        try
        {
            PixelFrame frame = CaptureService.Capture(region);
            if (candidate is not null && ScrollingCaptureStitcher.AreSimilar(candidate, frame))
            {
                candidate = null;
                if (awaitingRetainedViewport)
                {
                    if (!stitcher.MatchesCurrentViewport(frame)) return;
                    awaitingRetainedViewport = false;
                }
                ScrollAppendResult result = Append(frame);
                if (autoScrolling)
                {
                    unchangedScrolls = result == ScrollAppendResult.Unchanged ? unchangedScrolls + 1 : 0;
                    if (unchangedScrolls >= 3)
                    {
                        SetAutomatic(false);
                        ShowStatus("画面未继续滚动，可完成", "也可以继续使用鼠标滚轮手动截取。");
                    }
                    else ScrollDown();
                }
            }
            else candidate = frame;
        }
        catch (Exception error) when (IsCaptureError(error)) { CaptureFailed(error); }
    }

    private void ToggleAutomatic()
    {
        if (stopped || finishing || !excluded) return;
        if (awaitingRetainedViewport) { ShowStatus("先往回滚到保留的位置"); return; }
        if (limitReached) return;
        candidate = null;
        finishRetained = false;
        unchangedScrolls = 0;
        SetAutomatic(!autoScrolling);
        if (!autoScrolling) { ShowStatus("滚动鼠标滚轮截取"); return; }
        capturePaused = false; timer.Start();
        ShowStatus("正在自动向下滚动");
        ScrollDown();
    }

    private void ScrollDown()
    {
        try { input.ScrollDown(); }
        catch (Exception error) when (IsCaptureError(error))
        {
            SetAutomatic(false);
            ShowStatus("请用鼠标滚轮继续截取", error.Message);
            System.Diagnostics.Trace.TraceError("Automatic scrolling failed: {0}", error);
        }
    }

    private void SetAutomatic(bool value)
    {
        autoScrolling = value;
        automaticButton.Content = VectorIcon.Create(value ? "pause" : "chevronDown", 19, foreground);
        automaticButton.ToolTip = value ? "停止自动滚动" : "自动向下滚动";
        System.Windows.Automation.AutomationProperties.SetName(automaticButton, (string)automaticButton.ToolTip);
    }

    private ScrollAppendResult Append(PixelFrame frame)
    {
        ScrollAppendResult result = stitcher.TryAppend(frame);
        if (result is ScrollAppendResult.Appended or ScrollAppendResult.Unchanged) finishRetained = false;
        if (result is ScrollAppendResult.NoReliableOverlap or ScrollAppendResult.LimitReached) SetAutomatic(false);
        ShowStatus(result switch
        {
            ScrollAppendResult.LimitReached => "已达长度上限，点击完成",
            ScrollAppendResult.NoReliableOverlap => finishRetained ? "末屏未拼接，再点完成保留" : "未能拼接，请往回滚一点",
            _ => autoScrolling ? "正在自动向下滚动" : "滚动鼠标滚轮截取"
        }, result == ScrollAppendResult.NoReliableOverlap ? "已保留之前的内容。向下慢慢滚动，避开动画和固定底栏。" : Dimensions());
        if (result == ScrollAppendResult.LimitReached)
        {
            limitReached = true;
            timer.Stop(); automaticButton.IsEnabled = false;
        }
        return result;
    }

    private void Undo()
    {
        if (finishing || stopped) return;
        SetAutomatic(false);
        candidate = null;
        if (!stitcher.Undo()) return;
        capturePaused = limitReached = finishRetained = false;
        awaitingRetainedViewport = true;
        ShowStatus("已撤销，请往回滚一点");
        if (excluded) { automaticButton.IsEnabled = true; timer.Start(); }
    }

    /// <summary>Completes the capture from the hosting overlay, which owns the keyboard while it is focused.</summary>
    internal void RequestFinish() => _ = FinishAsync();

    private async Task FinishAsync()
    {
        if (stopped || finishing) return;
        finishing = true; timer.Stop(); SetAutomatic(false);
        finishButton.IsEnabled = automaticButton.IsEnabled = false;
        ShowStatus("正在完成…");
        try
        {
            // Include the final settled viewport even when Complete precedes the next timer tick.
            if (excluded && !capturePaused && !limitReached && !finishRetained && !awaitingRetainedViewport)
            {
                PixelFrame last = CaptureService.Capture(region);
                await Task.Delay(160);
                if (stopped) return;
                PixelFrame settled = CaptureService.Capture(region);
                if (!ScrollingCaptureStitcher.AreSimilar(last, settled))
                {
                    finishRetained = true;
                    ShowStatus("画面未停稳，再点完成保留", "可等待画面停稳后继续截取，或再次点击完成，保存已经截取的内容。");
                    return;
                }
                if (Append(settled) == ScrollAppendResult.NoReliableOverlap)
                {
                    finishRetained = true;
                    ShowStatus("末屏未拼接，再点完成保留", "最后一屏未加入。可往回滚到有重叠的位置，或再次点击完成，保存已经截取的内容。");
                    return;
                }
            }
            Result = CaptureService.ToBitmapSource(stitcher.CreateFrame());
            Close();
        }
        catch (Exception error) when (IsCaptureError(error) || error is OutOfMemoryException)
        {
            capturePaused = true;
            ShowStatus("完成失败，可重试或撤销", error.Message);
            System.Diagnostics.Trace.TraceError("Completing scrolling capture failed: {0}", error);
        }
        finally
        {
            finishing = false;
            if (!stopped)
            {
                finishButton.IsEnabled = true; automaticButton.IsEnabled = excluded && !limitReached;
                if (excluded && !capturePaused && !limitReached) timer.Start();
            }
        }
    }

    private void CaptureFailed(Exception error)
    {
        SetAutomatic(false); timer.Stop();
        capturePaused = true;
        candidate = null;
        ShowStatus("已暂停，可重试或完成", error.Message);
        System.Diagnostics.Trace.TraceError("Scrolling capture failed: {0}", error);
    }

    private string Dimensions() => $"{stitcher.Width} × {stitcher.Height} · {(cancelHotkeyRegistered ? "Esc 取消" : "点击叉号取消")} · 浮条聚焦时 Enter 完成、Ctrl+Z 撤销上屏";
    private void ShowStatus(string message, string? detail = null) { status.Text = message; status.ToolTip = detail ?? Dimensions(); }
    private static bool IsCaptureError(Exception error) => error is Win32Exception or InvalidOperationException or ArgumentException or OverflowException or COMException or OutOfMemoryException;
    private static SolidColorBrush Brush(string color) => new((Color)ColorConverter.ConvertFromString(color));

    private Button Action(string icon, string label, Action action, bool dark)
    {
        var button = AnnotationChrome.Button(VectorIcon.Create(icon, 19, foreground), label, dark);
        button.Click += (_, _) => action();
        return button;
    }

    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool RegisterHotKey(nint window, int id, uint modifiers, uint key);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool UnregisterHotKey(nint window, int id);
}

internal static class ScrollingCapturePlacement
{
    [StructLayout(LayoutKind.Sequential)] private struct NativePoint { public int X; public int Y; }
    [StructLayout(LayoutKind.Sequential)] private struct NativeRect { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct MonitorInfo { public int Size; public NativeRect Monitor; public NativeRect Work; public uint Flags; }
    [DllImport("user32.dll")] private static extern nint MonitorFromPoint(NativePoint point, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetMonitorInfo(nint monitor, ref MonitorInfo info);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetWindowRect(nint window, out NativeRect rect);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(nint window, nint after, int x, int y, int width, int height, uint flags);

    private static NativeRect WorkArea(Point point)
    {
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (!GetMonitorInfo(MonitorFromPoint(new NativePoint { X = (int)point.X, Y = (int)point.Y }, 2), ref info))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "无法读取显示器工作区。");
        return info.Work;
    }

    public static void Place(Window window, PixelRect region, Point? position = null)
    {
        NativeRect work = WorkArea(position ?? new Point(region.Right - 1, region.Y));
        DpiScale dpi = VisualTreeHelper.GetDpi(window);
        int width = (int)Math.Ceiling(window.Width * dpi.DpiScaleX);
        int height = (int)Math.Ceiling(window.Height * dpi.DpiScaleY);
        int x = (int)Math.Round(position?.X ?? region.Right - width);
        int y = (int)Math.Round(position?.Y ?? region.Y - height - 12);
        if (position == null && y < work.Top)
        {
            y = region.Bottom + 12;
            if (y + height > work.Bottom) y = region.Y + 8;
        }
        PlaceClamped(window, work, x, y, width, height);
    }

    public static void Constrain(Window window)
    {
        if (!GetWindowRect(new WindowInteropHelper(window).Handle, out NativeRect rect))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "无法读取滚动截图工具栏位置。");
        PlaceClamped(window, WorkArea(new Point(rect.Left + 16, rect.Top + 16)), rect.Left, rect.Top, rect.Right - rect.Left, rect.Bottom - rect.Top);
    }

    private static void PlaceClamped(Window window, NativeRect work, int x, int y, int width, int height)
    {
        width = Math.Min(width, work.Right - work.Left); height = Math.Min(height, work.Bottom - work.Top);
        x = Math.Clamp(x, work.Left, Math.Max(work.Left, work.Right - width));
        y = Math.Clamp(y, work.Top, Math.Max(work.Top, work.Bottom - height));
        if (!SetWindowPos(new WindowInteropHelper(window).Handle, new nint(-1), x, y, width, height, 0x0010))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "无法放置滚动截图工具栏。");
    }
}
