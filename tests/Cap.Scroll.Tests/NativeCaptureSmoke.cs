using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using Cap.Annotation;
using Cap.Capture;
using Cap.Core;

internal static class NativeCaptureSmoke
{
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetWindowRect(nint handle, out Rect bounds);

    public static void Run()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            var app = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
            var pixels = new byte[220 * 1000 * 4];
            new Random(20261024).NextBytes(pixels);
            for (int p = 3; p < pixels.Length; p += 4) pixels[p] = 255;
            var image = new Image { Source = CaptureService.ToBitmapSource(new PixelFrame(220, 1000, pixels)), Stretch = Stretch.None,
                HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top };
            var window = new Window
            {
                Title = "Cap scrolling capture native smoke", Width = 220, Height = 240, Left = SystemParameters.WorkArea.Left + 40,
                Top = SystemParameters.WorkArea.Top + 40, WindowStyle = WindowStyle.None, ResizeMode = ResizeMode.NoResize,
                ShowInTaskbar = false, Topmost = true,
                Content = new Canvas { ClipToBounds = true, Children = { image } }
            };
            Window? overlay = null;
            PixelFrame? first = null;
            PixelRect bounds = default;
            int stage = 0;
            var timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
            timer.Tick += (_, _) =>
            {
                try
                {
                    if (stage == 0)
                    {
                        if (!GetWindowRect(new WindowInteropHelper(window).Handle, out Rect rect)) throw new InvalidOperationException("GetWindowRect failed.");
                        bounds = new(rect.Left, rect.Top, rect.Right - rect.Left, rect.Bottom - rect.Top);
                        first = CaptureService.Capture(bounds);
                        overlay = new Window { Width = 80, Height = 35, Left = window.Left + 10, Top = window.Top + 10,
                            WindowStyle = WindowStyle.None, ResizeMode = ResizeMode.NoResize, Topmost = true,
                            ShowInTaskbar = false, Background = Brushes.Magenta };
                        overlay.SourceInitialized += (_, _) => CaptureService.ExcludeWindow(new WindowInteropHelper(overlay).Handle);
                        overlay.Show();
                    }
                    else if (stage == 1)
                    {
                        PixelFrame excluded = CaptureService.Capture(bounds);
                        if (!first!.Pixels.SequenceEqual(excluded.Pixels)) throw new InvalidOperationException("Excluded control changed the captured target pixels.");
                        DpiScale dpi = VisualTreeHelper.GetDpi(window);
                        image.RenderTransform = new TranslateTransform(0, -Math.Round(80 * dpi.DpiScaleY) / dpi.DpiScaleY);
                    }
                    else
                    {
                        PixelFrame second = CaptureService.Capture(bounds);
                        var stitcher = new ScrollingCaptureStitcher(first!);
                        if (stitcher.TryAppend(second) != ScrollAppendResult.Appended) throw new InvalidOperationException("Native GDI viewports did not align.");
                        int expected = first!.Height + (int)Math.Round(80 * VisualTreeHelper.GetDpi(window).DpiScaleY);
                        if (stitcher.Height != expected) throw new InvalidOperationException($"Native stitched height {stitcher.Height}, expected {expected}.");
                        var bitmap = CaptureService.ToBitmapSource(stitcher.CreateFrame());
                        if (!bitmap.IsFrozen || bitmap.DpiX != 96 || bitmap.PixelHeight != expected) throw new InvalidOperationException("Final bitmap contract failed.");
                        timer.Stop(); overlay?.Close();
                        VerifyWorkflow(bounds, image, window);
                        window.Close(); app.Shutdown();
                    }
                    stage++;
                }
                catch (Exception error)
                {
                    failure = error;
                    timer.Stop(); overlay?.Close(); window.Close(); app.Shutdown();
                }
            };
            window.Loaded += (_, _) => timer.Start();
            window.Show();
            app.Run();
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.IsBackground = true;
        thread.Start();
        if (!thread.Join(TimeSpan.FromSeconds(15))) throw new TimeoutException("Native capture smoke timed out.");
        if (failure is not null) throw new InvalidOperationException("Native capture smoke failed.", failure);
        Console.WriteLine("PASS: native GDI captures stitched after owned-page scrolling; excluded overlay absent; frozen 96-DPI output.");
        Console.WriteLine("PASS: real scrolling workflow captured the next viewport, completed, returned frozen output, and cancelled cleanly.");
    }

    private static void VerifyWorkflow(PixelRect bounds, Image image, Window target)
    {
        int step = 0;
        Exception? driverFailure = null;
        var driver = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(450) };
        driver.Tick += (_, _) =>
        {
            Window? control = Application.Current.Windows.OfType<Window>().FirstOrDefault(item => item.Title == "Cap 滚动截图");
            if (control is null) return;
            try
            {
                if (step == 0)
                {
                    DpiScale dpi = VisualTreeHelper.GetDpi(target);
                    image.RenderTransform = new TranslateTransform(0, -Math.Round(160 * dpi.DpiScaleY) / dpi.DpiScaleY);
                }
                // Allow the default settled-frame capture to append without a manual capture action.
                else if (step >= 3) { driver.Stop(); Click(control, "完成"); }
                step++;
            }
            catch (Exception error) { driverFailure = error; driver.Stop(); control.Close(); }
        };
        driver.Start();
        var output = AwaitWorkflow(ScrollingCaptureWorkflow.RunAsync(bounds, Environment.GetFolderPath(Environment.SpecialFolder.MyPictures)));
        driver.Stop();
        if (driverFailure is not null) throw driverFailure;
        int expected = bounds.Height + (int)Math.Round(80 * VisualTreeHelper.GetDpi(target).DpiScaleY);
        if (output is null || output.PixelHeight != expected || !output.IsFrozen) throw new InvalidOperationException("Real workflow did not return the stitched image.");

        var cancelDriver = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(450) };
        cancelDriver.Tick += (_, _) =>
        {
            Window? control = Application.Current.Windows.OfType<Window>().FirstOrDefault(item => item.Title == "Cap 滚动截图");
            if (control is not null) { cancelDriver.Stop(); Click(control, "取消"); }
        };
        cancelDriver.Start();
        var cancelled = AwaitWorkflow(ScrollingCaptureWorkflow.RunAsync(bounds, Environment.GetFolderPath(Environment.SpecialFolder.MyPictures)));
        cancelDriver.Stop();
        if (cancelled is not null) throw new InvalidOperationException("Cancelling must return null.");
    }

    private static T AwaitWorkflow<T>(Task<T> task)
    {
        if (!task.IsCompleted)
        {
            var dispatcher = Dispatcher.CurrentDispatcher;
            var frame = new DispatcherFrame();
            _ = task.ContinueWith(_ => dispatcher.BeginInvoke(new Action(() => frame.Continue = false)), TaskScheduler.Default);
            Dispatcher.PushFrame(frame);
        }
        return task.GetAwaiter().GetResult();
    }

    private static void Click(DependencyObject root, string label)
    {
        Button? button = FindButton(root, label);
        if (button is null) throw new InvalidOperationException($"Missing workflow action: {label}");
        button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
    }

    private static Button? FindButton(DependencyObject node, string label)
    {
        if (node is Button button && System.Windows.Automation.AutomationProperties.GetName(button) == label) return button;
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(node); i++)
        {
            Button? found = FindButton(VisualTreeHelper.GetChild(node, i), label);
            if (found is not null) return found;
        }
        return null;
    }
}
