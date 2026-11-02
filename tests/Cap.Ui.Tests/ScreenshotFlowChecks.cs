using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Cap.Annotation;
using Cap.Core;

internal static class ScreenshotFlowChecks
{
    internal static void Run(bool dark, string output)
    {
        var type = typeof(ScreenshotWorkflow).Assembly.GetType("Cap.Annotation.ScreenshotWindow")!;
        byte[] pixels = new byte[1280 * 720 * 4];
        for (int y = 0; y < 720; y++) for (int x = 0; x < 1280; x++)
        {
            int offset = (y * 1280 + x) * 4;
            byte tone = (byte)(x % 64 == 0 || y % 64 == 0 ? 220 : 244);
            pixels[offset] = pixels[offset + 1] = pixels[offset + 2] = tone; pixels[offset + 3] = 255;
        }
        Window Create(CaptureMode mode) => (Window)Activator.CreateInstance(type, [new PixelRect(0, 0, 1280, 720), new PixelFrame(1280, 720, pixels), Array.Empty<DesktopWindow>(), output, dark, false, mode])!;
        var window = Create(CaptureMode.Screenshot);
        var originalClipboard = Clipboard.GetDataObject();
        try
        {
            window.Show(); Pump(window);
            var modeBar = (FrameworkElement)type.GetField("modeBar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(window)!;
            var toolbar = (FrameworkElement)type.GetField("toolbar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(window)!;
            void Select(PixelRect bounds)
            {
                type.GetProperty("Selection")!.SetValue(window, bounds);
                type.GetMethod("Update", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(window, null); Pump(window);
                if (modeBar.Visibility != Visibility.Visible || toolbar.Visibility != Visibility.Visible)
                    throw new Exception("Function picker and annotation toolbar must remain visible together.");
                Rect upper = Bounds(modeBar), lower = Bounds(toolbar);
                if (upper.IntersectsWith(lower) || upper.Left < 0 || lower.Left < 0 || upper.Top < 0 || lower.Top < 0 || upper.Right > window.ActualWidth || lower.Right > window.ActualWidth || upper.Bottom > window.ActualHeight || lower.Bottom > window.ActualHeight)
                    throw new Exception("Both screenshot bars must stay inside the display without overlapping.");
            }
            foreach (var bounds in new[] { new PixelRect(5, 5, 200, 120), new PixelRect(10, 650, 180, 60), new PixelRect(0, 0, 1280, 720), new PixelRect(1200, 680, 60, 30) }) Select(bounds);
            Select(new PixelRect(140, 180, 900, 350));
            double scaleY = window.ActualHeight / 720;
            if (Bounds(modeBar).Bottom > 180 * scaleY || Bounds(toolbar).Top < 530 * scaleY)
                throw new Exception($"The function picker belongs above the selection and annotation tools below it when space is available. Window={window.ActualWidth}x{window.ActualHeight}, upper={Bounds(modeBar)}, lower={Bounds(toolbar)}.");
            var rectangle = Buttons(toolbar).Single(button => (string?)button.ToolTip == "矩形");
            double compactHeight = toolbar.ActualHeight;
            rectangle.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(window);
            if (toolbar.ActualHeight <= compactHeight + 20)
                throw new Exception("Selecting a drawing tool must expand the contextual options row, matching Feishu.");
            if ((AnnotationTool)type.GetProperty("Tool")!.GetValue(window)! != AnnotationTool.Rectangle || modeBar.Visibility != Visibility.Visible)
                throw new Exception("Rectangle annotation must be directly available without dismissing the function picker.");
            var toolButtons = Buttons(toolbar);
            if (!toolButtons.Any(button => ((string?)button.ToolTip)?.StartsWith("提取文字", StringComparison.Ordinal) == true))
                throw new Exception("The toolbar must expose the inline OCR button, matching Feishu.");
            if (!toolButtons.Any(button => ((string?)button.ToolTip)?.StartsWith("自动滚动", StringComparison.Ordinal) == true))
                throw new Exception("The toolbar must expose the auto-scroll button, matching Feishu.");
            if (!toolButtons.Any(button => (string?)button.ToolTip == "细") || !toolButtons.Any(button => (string?)button.ToolTip == "粗"))
                throw new Exception("The options row must expose thin/medium/thick stroke presets.");
            foreach (string name in new[] { "红色", "黄色", "绿色", "蓝色", "深灰", "灰色", "白色" })
                if (!toolButtons.Any(button => (string?)button.ToolTip == name))
                    throw new Exception("The options row must expose the seven-colour palette; missing " + name + ".");
            Buttons(toolbar).Single(button => (string?)button.ToolTip == "蓝色").RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(window);
            // The palette ramp is theme-specific: Feishu brightens the blue in dark mode.
            string expectedBlue = dark ? "#4C88FF" : "#336DF4";
            if ((Color)type.GetProperty("InkColor")!.GetValue(window)! != (Color)ColorConverter.ConvertFromString(expectedBlue))
                throw new Exception("A palette choice must apply to the active tool.");
            Buttons(toolbar).Single(button => (string?)button.ToolTip == "粗").RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(window);
            if (Math.Abs((double)type.GetProperty("InkThickness")!.GetValue(window)! - 6) > .01)
                throw new Exception("A stroke preset must apply line thickness.");
            Buttons(toolbar).Single(button => (string?)button.ToolTip == "标注").RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(window);
            if ((AnnotationTool)type.GetProperty("Tool")!.GetValue(window)! != AnnotationTool.Marker)
                throw new Exception("The 标注 (highlighter) tool must be selectable, matching Feishu.");
            if (!VisualTree(toolbar).OfType<Slider>().Any(slider => Math.Abs(slider.Value - 30) < 1))
                throw new Exception("The 标注 tool must expose Feishu's 30% opacity slider.");
            Buttons(toolbar).Single(button => (string?)button.ToolTip == "文字").RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(window);
            if (!Buttons(toolbar).Any(button => (string?)button.ToolTip == "字号"))
                throw new Exception("Text and number tools must expose a font-size option, matching Feishu.");
            var document = type.GetProperty("Document")!.GetValue(window)!;
            var command = Activator.CreateInstance(typeof(ScreenshotWorkflow).Assembly.GetType("Cap.Annotation.PaintCommand")!, [AnnotationTool.Rectangle, new Point(220, 250), new Point(470, 380), Colors.Red, 4d, null, null, 0, null])!;
            document.GetType().GetMethod("Add")!.Invoke(document, [command]);
            type.GetMethod("Update", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(window, null); Pump(window);
            var bitmap = new RenderTargetBitmap(1280, 720, 1280 / window.ActualWidth * 96, 720 / window.ActualHeight * 96, PixelFormats.Pbgra32); bitmap.Render(window);
            var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap));
            using (var stream = File.Create(Path.Combine(output, "screenshot-flow-" + (dark ? "dark" : "light") + ".png"))) encoder.Save(stream);
            Buttons(toolbar).Single(button => ((string?)button.ToolTip)?.StartsWith("复制完成", StringComparison.Ordinal) == true).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            var watch = System.Diagnostics.Stopwatch.StartNew();
            while (type.GetProperty("Result")!.GetValue(window) == null && watch.Elapsed < TimeSpan.FromSeconds(2)) { Pump(window); Thread.Sleep(10); }
            var result = (ScreenshotResult?)type.GetProperty("Result")!.GetValue(window);
            if (result?.Copied != true || result.Bounds != new PixelRect(140, 180, 900, 350) || Clipboard.GetImage() is not BitmapSource copied || copied.PixelWidth != 900 || copied.PixelHeight != 350)
                throw new Exception("One click on annotation completion must copy the selected screenshot.");
            byte[] pixel = new byte[4]; result.Image.CopyPixels(new Int32Rect(80, 100, 1, 1), pixel, 4, 0);
            if (pixel[2] < 200 || pixel[1] > 40) throw new Exception("The copied screenshot lost the rectangle annotation.");
        }
        finally { window.Close(); if (originalClipboard != null) Clipboard.SetDataObject(originalClipboard, true); else Clipboard.Clear(); }
        var recording = Create(CaptureMode.Recording);
        try
        {
            recording.Show(); Pump(recording);
            type.GetProperty("Selection")!.SetValue(recording, new PixelRect(140, 180, 900, 350));
            type.GetMethod("Update", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(recording, null); Pump(recording);
            var bar = (FrameworkElement)type.GetField("modeBar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(recording)!;
            Buttons(bar).Single(button => (string?)button.Content == "截图").RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(recording);
            if (((FrameworkElement)type.GetField("toolbar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(recording)!).Visibility != Visibility.Visible || bar.Visibility != Visibility.Visible)
                throw new Exception("Switching from recording to screenshot must preserve both bars.");
            Buttons(bar).Single(button => (string?)button.Content == "录屏").RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            var result = (ScreenshotResult?)type.GetProperty("Result")!.GetValue(recording);
            if (result?.Mode != CaptureMode.Recording || result.Bounds != new PixelRect(140, 180, 900, 350))
                throw new Exception("The function picker must still route the selected region to recording.");
        }
        finally { recording.Close(); }
        // Crop tool reopens a smaller overlay covering exactly the selected region (Feishu parity).
        var crop = Create(CaptureMode.Screenshot);
        try
        {
            crop.Show(); Pump(crop);
            type.GetProperty("Selection")!.SetValue(crop, new PixelRect(140, 180, 900, 350));
            type.GetMethod("Update", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(crop, null); Pump(crop);
            Buttons((FrameworkElement)type.GetField("toolbar", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(crop)!)
                .Single(button => ((string?)button.ToolTip)?.StartsWith("裁剪", StringComparison.Ordinal) == true)
                .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            var cropRegion = (PixelRect?)type.GetProperty("CropRegion")!.GetValue(crop);
            var croppedFrame = type.GetProperty("CroppedFrame")!.GetValue(crop);
            if (cropRegion != new PixelRect(40, 130, 900, 350))
                throw new Exception("Crop must reopen the overlay over the selected region; got " + cropRegion + ".");
            if (croppedFrame is null || (int)croppedFrame.GetType().GetProperty("Width")!.GetValue(croppedFrame)! != 900
                || (int)croppedFrame.GetType().GetProperty("Height")!.GetValue(croppedFrame)! != 350)
                throw new Exception("Crop must hand the workflow the baked region pixels.");
        }
        finally { crop.Close(); }
        Console.WriteLine("PASS: upper function picker + lower annotation tools, edge layouts, actual annotation button, one-click annotated image copy, recording-mode route and window-shrinking crop.");
    }
    private static System.Collections.Generic.IEnumerable<DependencyObject> VisualTree(DependencyObject root)
    {
        int count = System.Windows.Media.VisualTreeHelper.GetChildrenCount(root);
        for (int index = 0; index < count; index++)
        {
            DependencyObject child = System.Windows.Media.VisualTreeHelper.GetChild(root, index);
            yield return child;
            foreach (var nested in VisualTree(child)) yield return nested;
        }
    }

    private static Rect Bounds(FrameworkElement element) => new(Canvas.GetLeft(element), Canvas.GetTop(element), element.ActualWidth, element.ActualHeight);
    private static System.Collections.Generic.IEnumerable<Button> Buttons(DependencyObject root)
    {
        foreach (object child in LogicalTreeHelper.GetChildren(root)) if (child is DependencyObject dependency)
        {
            if (dependency is Button button) yield return button;
            foreach (var nested in Buttons(dependency)) yield return nested;
        }
    }
    /// <summary>Bounded dispatcher pump: processes queued work (async handlers, layout)
    /// without waiting for a global idle state that may never be reached.</summary>
    private static void Pump(Window window)
    {
        var frame = new DispatcherFrame();
        var timer = new DispatcherTimer(TimeSpan.FromMilliseconds(150), DispatcherPriority.Send,
            (_, _) => frame.Continue = false, window.Dispatcher);
        timer.Start();
        window.Dispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, new Action(() => frame.Continue = false));
        Dispatcher.PushFrame(frame);
        timer.Stop();
        window.UpdateLayout();
    }
}
