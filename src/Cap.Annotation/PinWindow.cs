using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cap.Capture;
using Cap.Core;
using Cap.Visuals;

namespace Cap.Annotation;

internal sealed class PinWindow : Window
{
    private readonly BitmapSource image;
    private readonly double originalWidth;
    private readonly double originalHeight;

    public PinWindow(BitmapSource image, PixelRect bounds)
    {
        this.image = image;
        Title = "Cap 贴图";
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.CanResizeWithGrip;
        Topmost = true;
        ShowInTaskbar = false;
        Background = Brushes.Black;
        MinWidth = 64;
        MinHeight = 48;
        MaxWidth = SystemParameters.VirtualScreenWidth;
        MaxHeight = SystemParameters.VirtualScreenHeight;
        originalWidth = Width = Math.Max(64, Math.Min(image.PixelWidth, 1000));
        originalHeight = Height = originalWidth * image.PixelHeight / image.PixelWidth;
        Left = bounds.X;
        Top = bounds.Y;
        Content = new Border { BorderBrush = AnnotationChrome.Accent, BorderThickness = new Thickness(1), Child = new Image { Source = image, Stretch = Stretch.Uniform } };
        ToolTip = "拖动移动 · 滚轮缩放 · 双击恢复 · 右键菜单 · Esc 关闭";
        MouseLeftButtonDown += (_, e) =>
        {
            if (e.ClickCount == 2) { Width = originalWidth; Height = originalHeight; }
            else DragMove();
        };
        MouseWheel += (_, e) =>
        {
            double factor = e.Delta > 0 ? 1.1 : 1 / 1.1;
            double next = Math.Clamp(Width * factor, MinWidth, MaxWidth);
            Height = Math.Clamp(next * image.PixelHeight / image.PixelWidth, MinHeight, MaxHeight);
            Width = next;
            e.Handled = true;
        };
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        var menu = new ContextMenu { Background = AnnotationChrome.Surface(false), Foreground = AnnotationChrome.Foreground(false), BorderBrush = AnnotationChrome.Brush("#E6E2E8"), Padding = new Thickness(5) };
        var copy = new MenuItem { Header = "复制图片", Icon = VectorIcon.Create("copy", 16, AnnotationChrome.Foreground(false)), Padding = new Thickness(8, 7, 12, 7) };
        copy.Click += (_, _) => { try { Clipboard.SetImage(this.image); } catch (System.Runtime.InteropServices.ExternalException ex) { MessageBox.Show(this, $"剪贴板暂时不可用：{ex.Message}", "Cap"); } };
        var close = new MenuItem { Header = "关闭贴图", Icon = VectorIcon.Create("close", 16, AnnotationChrome.Foreground(false)), Padding = new Thickness(8, 7, 12, 7), InputGestureText = "Esc" };
        System.Windows.Automation.AutomationProperties.SetName(copy, "复制图片");
        System.Windows.Automation.AutomationProperties.SetName(close, "关闭贴图");
        close.Click += (_, _) => Close();
        menu.Items.Add(copy);
        menu.Items.Add(new Separator());
        menu.Items.Add(close);
        ContextMenu = menu;
        SourceInitialized += (_, _) => CaptureService.ExcludeWindow(new System.Windows.Interop.WindowInteropHelper(this).Handle);
        Loaded += (_, _) =>
        {
            PixelRect monitor = NativeDesktop.MonitorAt(new Point(bounds.X, bounds.Y));
            double fit = Math.Min(1, Math.Min(monitor.Width * .9 / image.PixelWidth, monitor.Height * .9 / image.PixelHeight));
            int width = Math.Max(64, (int)Math.Round(image.PixelWidth * fit));
            int height = Math.Max(48, (int)Math.Round(image.PixelHeight * fit));
            NativeDesktop.Place(this, new PixelRect(Math.Clamp(bounds.X, monitor.X, monitor.Right - width), Math.Clamp(bounds.Y, monitor.Y, monitor.Bottom - height), width, height));
        };
    }
}
