using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Diagnostics;
using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using System.Windows.Input;
using System.Windows.Interop;
using Cap.Core;
using Cap.Platform;

internal static class Program
{
    private static readonly Assembly AppAssembly = typeof(Cap.App.App).Assembly;
    private static readonly Assembly AnnotationAssembly = typeof(Cap.Annotation.ScreenshotWorkflow).Assembly;
    private static bool verifiedVideo;
    [STAThread]
    private static int Main(string[] args)
    {
        // Off-screen render checks hang on some desktops when WPF waits on the GPU
        // composition thread; software rendering keeps the checks deterministic.
        System.Windows.Media.RenderOptions.ProcessRenderMode = System.Windows.Interop.RenderMode.SoftwareOnly;
        return Run(args);
    }

    private static int Run(string[] args)
    {
        string output = args.Length == 0 ? Path.Combine(AppContext.BaseDirectory, "interactive-smoke") : Path.GetFullPath(args[0]); Directory.CreateDirectory(output);
        var app = new UiTestApplication { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        app.Resources.MergedDictionaries.Add(new ResourceDictionary { Source = new Uri("pack://application:,,,/cap;component/Themes/Controls.xaml") });
        if (args.Contains("--preview")) return DeveloperPreview.Run(app, output);
        if (args.Length == 0 || args.Contains("--interactive")) return InteractiveScreenshotSmoke.Run(app, output);
        try
        {
            AppAssembly.GetType("Cap.App.App")!.GetMethod("CreateTray", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(app, null);
            var tray = (System.Windows.Forms.NotifyIcon)AppAssembly.GetType("Cap.App.App")!.GetField("tray", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(app)!;
            if (!tray.ContextMenuStrip!.Items.Cast<System.Windows.Forms.ToolStripItem>().Select(item => item.Text).SequenceEqual(new[] { "截屏", "录制", "重新注册快捷键", "设置", "退出" }))
                throw new Exception("Tray must expose exactly 截屏 / 录制 / 重新注册快捷键 / 设置 / 退出.");
            foreach (bool dark in new[] { false, true })
            {
                string theme = dark ? "dark" : "light";
                AppAssembly.GetType("Cap.App.Ui")!.GetMethod("Theme")!.Invoke(null, [dark]);
                var settings = new AppSettings { Theme = dark ? "Dark" : "Light" };
                var expectedBackground = (Color)ColorConverter.ConvertFromString(dark ? "#131315" : "#F0EDF1");
                if (((SolidColorBrush)app.Resources["WindowBrush"]).Color != expectedBackground)
                    throw new Exception("UI rendering unexpectedly changed the requested " + theme + " theme.");
                var advanced = Create("RecordingSetupWindow", settings, RecordingFormat.Gif);
                var expander = Descendants(advanced).OfType<Expander>().Single(); expander.IsExpanded = true;
                string video = Path.GetFullPath(".artifacts/portable-final/smoke/recording.mp4");
            }
            string iconPath = Path.GetFullPath("src/Cap.App/Assets/cap.ico");
            using (var reader = new BinaryReader(File.OpenRead(iconPath)))
            {
                if (reader.ReadUInt16() != 0 || reader.ReadUInt16() != 1 || reader.ReadUInt16() != 9) throw new Exception("App ICO must contain nine real resolutions.");
            }
            string executable = Path.GetFullPath("src/Cap.App/bin/Release/net8.0-windows10.0.19041.0/win-x64/cap.exe");
            using (var icon = System.Drawing.Icon.ExtractAssociatedIcon(executable))
            using (var bitmap = icon!.ToBitmap())
            {
                int white = 0, blue = 0;
                for (int y = 0; y < bitmap.Height; y++) for (int x = 0; x < bitmap.Width; x++)
                {
                    var color = bitmap.GetPixel(x, y);
                    if (color.A > 200 && color.R > 210 && color.G > 210 && color.B > 210) white++;
                    if (color.B > 170 && color.R < 150) blue++;
                }
                if (white < 3 || blue < 10 || bitmap.GetPixel(0, 0).A > 0) throw new Exception("Executable did not contain the blue rounded screenshot icon with transparent corners.");
                bitmap.Save(Path.Combine(output, "extracted-exe-icon.png"));
            }
            Console.WriteLine("PASS: simultaneous upper function picker and lower annotation tools, annotated one-click copy, recording route, first Ctrl+C from both initial modes, tray with Exit, WPF light/dark renders, OCR result UI and EXE icon. MP4 open/seek checked: " + verifiedVideo + ". " + output);
            return 0;
        }
        catch (Exception ex) { Console.Error.WriteLine(ex); return 1; }
        finally { app.Shutdown(); }
    }
    private static Window Create(string name, params object[] arguments) => (Window)Activator.CreateInstance(AppAssembly.GetType("Cap.App." + name)!, arguments)!;
    private sealed class UiTestApplication : Cap.App.App
    {
        // Render fixtures without loading user settings, registering hotkeys or enabling startup.
        protected override void OnStartup(StartupEventArgs e) { }
    }
    private static void CopyShortcut()
    {
        var previousClipboard = Clipboard.GetDataObject();
        try
        {
            using var source = new HwndSource(new HwndSourceParameters("cap-shortcut-test") { Width = 1, Height = 1, PositionX = -12000, PositionY = -12000, WindowStyle = 0 });
            Type type = AnnotationAssembly.GetType("Cap.Annotation.ScreenshotWindow")!;
            foreach (var initialMode in new[] { Cap.Annotation.CaptureMode.Screenshot, Cap.Annotation.CaptureMode.Recording })
            {
                var window = (Window)Activator.CreateInstance(type, [new PixelRect(-100, -50, 64, 64), new PixelFrame(64, 64, new byte[64 * 64 * 4]), Array.Empty<DesktopWindow>(), Path.GetTempPath(), false, false, initialMode])!;
                try
                {
                    type.GetProperty("Selection")!.SetValue(window, new PixelRect(3, 4, 20, 16));
                    var key = new KeyEventArgs(Keyboard.PrimaryDevice, source, Environment.TickCount, Key.C) { RoutedEvent = Keyboard.PreviewKeyDownEvent };
                    type.GetMethod("HandleKey", BindingFlags.NonPublic | BindingFlags.Instance)!.Invoke(window, [key, ModifierKeys.Control]);
                    var watch = Stopwatch.StartNew();
                    while (type.GetProperty("Result")!.GetValue(window) == null && watch.Elapsed < TimeSpan.FromSeconds(2))
                    { window.Dispatcher.Invoke(() => { }, DispatcherPriority.Send); Thread.Sleep(10); }
                    var result = (Cap.Annotation.ScreenshotResult?)type.GetProperty("Result")!.GetValue(window);
                    if (!key.Handled || result?.Copied != true || result.Mode != Cap.Annotation.CaptureMode.Screenshot || result.Bounds != new PixelRect(-97, -46, 20, 16))
                        throw new Exception("First Ctrl+C must copy the screenshot regardless of initial mode: " + initialMode);
                    if (Clipboard.GetImage() is not BitmapSource image || image.PixelWidth != 20 || image.PixelHeight != 16)
                        throw new Exception("Ctrl+C did not put the selected image on the clipboard.");
                }
                finally { window.Close(); }
            }
        }
        finally { if (previousClipboard != null) Clipboard.SetDataObject(previousClipboard, true); else Clipboard.Clear(); }
    }

    private static void RenderWindow(Window window, string path)
    {
        window.WindowStartupLocation = WindowStartupLocation.Manual; window.Left = -12000; window.Top = -12000;
        // TextBox hosts otherwise pull the IME/TSF stack into the off-screen render pass,
        // which can block the dispatcher on desktops with a third-party IME installed.
        System.Windows.Input.InputMethod.SetIsInputMethodEnabled(window, false);
        window.Show();
        // RenderTargetBitmap renders synchronously, so no dispatcher idle pump is required;
        // pumping off-screen windows can block forever on some desktops.
        window.UpdateLayout();
        if (!window.IsVisible || window.ActualWidth < 100) throw new Exception("Window did not load: " + window.Title);
        if (window.GetType().Name == "PreviewWindow")
        {
            var media = window.GetType().GetField("media", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(window) as MediaElement;
            if (media != null)
            {
                // The media engine can stall in this session (another app holding the
                // pipeline); verify when it responds and otherwise record the skip
                // instead of hanging the whole UI suite.
                var watch = Stopwatch.StartNew();
                while (!media.NaturalDuration.HasTimeSpan && watch.Elapsed < TimeSpan.FromSeconds(3)) Thread.Sleep(50);
                if (media.NaturalDuration.HasTimeSpan && media.NaturalVideoWidth > 0)
                {
                    media.Pause();
                    var slider = VisualDescendants(window).OfType<Slider>().Single(); slider.Value = Math.Min(.7, slider.Maximum / 2);
                    if (Math.Abs(media.Position.TotalSeconds - slider.Value) > .25) throw new Exception("Preview seek slider did not update playback position.");
                    verifiedVideo = true;
                }
                else
                {
                }
            }
        }
        Save(window, (int)Math.Ceiling(window.ActualWidth), (int)Math.Ceiling(window.ActualHeight), path);
        window.Close();
    }
    private static void Toolbar(bool dark, string path)
    {
        const int width = 1280, height = 720;
        byte[] pixels = new byte[width * height * 4];
        for (int i = 0; i < pixels.Length; i += 4) { pixels[i] = 240; pixels[i + 1] = 238; pixels[i + 2] = 240; pixels[i + 3] = 255; }
        Type windowType = AnnotationAssembly.GetType("Cap.Annotation.ScreenshotWindow")!;
        var window = (Window)Activator.CreateInstance(windowType, [new PixelRect(0, 0, width, height), new PixelFrame(width, height, pixels), Array.Empty<DesktopWindow>(), Path.GetTempPath(), dark, false])!;
        var chooser = (FrameworkElement)windowType.GetField("modeBar", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(window)!;
        if (chooser.Visibility != Visibility.Collapsed) throw new Exception("Chooser appeared before a region was selected.");
        windowType.GetProperty("Selection")!.SetValue(window, new PixelRect(50, 50, 1000, 500));
        var toolbar = (FrameworkElement)windowType.GetField("toolbar", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(window)!;
        windowType.GetMethod("Update", BindingFlags.NonPublic | BindingFlags.Instance)!.Invoke(window, null);
        if (chooser.Visibility != Visibility.Visible || toolbar.Visibility != Visibility.Visible || windowType.GetProperty("Result")!.GetValue(window) != null)
            throw new Exception("Selecting a screenshot region must show both the function picker and annotation tools without exporting.");
        ((Canvas)window.Content).Children.Remove(chooser);
        RenderWindow(new Window { Content = chooser, SizeToContent = SizeToContent.WidthAndHeight, WindowStyle = WindowStyle.None, ShowInTaskbar = false }, Path.Combine(Path.GetDirectoryName(path)!, "function-picker-" + (dark ? "dark" : "light") + ".png"));
        windowType.GetMethod("ChooseMode")!.Invoke(window, [Cap.Annotation.CaptureMode.Screenshot]);
        if (toolbar.Visibility != Visibility.Visible) throw new Exception("Screenshot mode did not reveal annotation tools.");
        ((Canvas)window.Content).Children.Remove(toolbar); toolbar.Visibility = Visibility.Visible;
        var host = new Window { Content = toolbar, SizeToContent = SizeToContent.WidthAndHeight, WindowStyle = WindowStyle.None, ShowInTaskbar = false, Background = Brushes.Transparent };
        RenderWindow(host, path);
        windowType.GetMethod("Record")!.Invoke(window, null);
        var result = (Cap.Annotation.ScreenshotResult?)windowType.GetProperty("Result")!.GetValue(window);
        if (result?.RecordRequested != true || result.Bounds != new PixelRect(50, 50, 1000, 500)) throw new Exception("Screenshot-to-recording route lost the physical selected bounds.");
        foreach (var mode in new[] { Cap.Annotation.CaptureMode.Scrolling, Cap.Annotation.CaptureMode.Text, Cap.Annotation.CaptureMode.Recording })
        {
            var choice = (Window)Activator.CreateInstance(windowType, [new PixelRect(-100, -50, width, height), new PixelFrame(width, height, pixels), Array.Empty<DesktopWindow>(), Path.GetTempPath(), dark, false])!;
            windowType.GetProperty("Selection")!.SetValue(choice, new PixelRect(50, 50, 1000, 500));
            windowType.GetMethod("ChooseMode")!.Invoke(choice, [mode]);
            if (mode == Cap.Annotation.CaptureMode.Scrolling)
            {
                // Feishu parity: scrolling capture shows an explicit start bar and only
                // begins scrolling after the analyst clicks it.
                var startBar = (FrameworkElement)windowType.GetField("scrollStartBar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(choice)!;
                bool scrolling = (bool)windowType.GetField("scrolling", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(choice)!;
                if (scrolling || startBar.Visibility != Visibility.Visible)
                    throw new Exception($"Scrolling capture must wait for the explicit start bar, matching Feishu (scrolling={scrolling}, visibility={startBar.Visibility}).");
                windowType.GetMethod("EnterScrollingMode", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(choice, null);
                if (!(bool)windowType.GetField("scrolling", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(choice)!)
                    throw new Exception("The scrolling start bar must arm the scrolling session when activated.");
                choice.Close();
                continue;
            }
            if (mode == Cap.Annotation.CaptureMode.Text)
            {
                // Feishu parity: OCR results appear inline instead of closing the overlay.
                var textPanel = (FrameworkElement)windowType.GetField("textPanel", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(choice)!;
                if (textPanel.Visibility != Visibility.Visible)
                    throw new Exception("Text extraction must open the inline OCR panel, matching Feishu.");
                windowType.GetMethod("CloseTextPanel", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(choice, null);
                if (textPanel.Visibility != Visibility.Collapsed)
                    throw new Exception("Closing the inline OCR panel must return to the overlay.");
                choice.Close();
                continue;
            }
            var selected = (Cap.Annotation.ScreenshotResult)windowType.GetProperty("Result")!.GetValue(choice)!;
            if (selected.Mode != mode || selected.Bounds != new PixelRect(-50, 0, 1000, 500)) throw new Exception("Mode dispatch lost its action or negative physical coordinates.");
        }
    }
    private static System.Collections.Generic.IEnumerable<DependencyObject> Descendants(DependencyObject root)
    {
        foreach (object child in LogicalTreeHelper.GetChildren(root))
            if (child is DependencyObject dependency) { yield return dependency; foreach (var nested in Descendants(dependency)) yield return nested; }
    }
    private static System.Collections.Generic.IEnumerable<DependencyObject> VisualDescendants(DependencyObject root)
    {
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var child = VisualTreeHelper.GetChild(root, i); yield return child;
            foreach (var nested in VisualDescendants(child)) yield return nested;
        }
    }
    private static void Save(Visual visual, int width, int height, string path)
    {
        var bitmap = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32); bitmap.Render(visual);
        var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap)); using var stream = File.Create(path); encoder.Save(stream);
    }
}
