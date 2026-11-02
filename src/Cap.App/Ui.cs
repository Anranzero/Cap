using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Shell;
using System.Windows.Input;
using System.Windows.Automation;
using Cap.Visuals;
using Cap.Core;
using Cap.Capture;
using System.Linq;

namespace Cap.App;

internal static class Ui
{
    private static bool isDark;
    private static BitmapImage? brand;
    private static BitmapImage BrandImage
    {
        get
        {
            if (brand != null) return brand;
            brand = new BitmapImage(new Uri("pack://application:,,,/cap;component/Assets/cap.png")); brand.Freeze(); return brand;
        }
    }
    public static void PrepareWindow(Window window)
    {
        window.SetResourceReference(Window.BackgroundProperty, "WindowBrush");
        window.SetResourceReference(Window.ForegroundProperty, "TextBrush");
        window.FontFamily = new FontFamily("Segoe UI,Microsoft YaHei UI"); window.FontSize = 13;
        window.WindowStyle = WindowStyle.None; window.Icon = BrandImage;
        window.SourceInitialized += (_, _) =>
        {
            if (!window.AllowsTransparency)
                WindowChrome.SetWindowChrome(window, new WindowChrome { CaptionHeight = 0, ResizeBorderThickness = new Thickness(window.ResizeMode == ResizeMode.CanResize ? 5 : 0), GlassFrameThickness = new Thickness(0), UseAeroCaptionButtons = false });
            ApplyTitleBar(window);
        };
    }
    private static void ApplyTitleBar(Window window)
    {
        nint handle = new WindowInteropHelper(window).Handle;
        if (handle == 0) return;
        int dark = isDark ? 1 : 0;
        if (DwmSetWindowAttribute(handle, 20, ref dark, sizeof(int)) != 0)
            DwmSetWindowAttribute(handle, 19, ref dark, sizeof(int));
        int rounded = 2; DwmSetWindowAttribute(handle, 33, ref rounded, sizeof(int));
    }
    public static void Theme(bool dark)
    {
        isDark = dark;
        var resources = Application.Current.Resources;
        resources["WindowBrush"] = Brush(dark ? "#131315" : "#F0EDF1");
        resources["SurfaceBrush"] = Brush(dark ? "#262627" : "#FCFAFC");
        resources["SidebarBrush"] = Brush(dark ? "#202022" : "#F5F2F6");
        resources["InputBrush"] = Brush(dark ? "#333335" : "#EFEDF1");
        resources["TextBrush"] = Brush(dark ? "#F3F2F6" : "#202126");
        resources["MutedBrush"] = Brush(dark ? "#A5A6B0" : "#777985");
        resources["BorderBrush"] = Brush(dark ? "#39393D" : "#E6E2E8");
        resources["AccentBrush"] = Brush("#087DFF");
        resources["AccentSoftBrush"] = Brush(dark ? "#263954" : "#E6F0FF");
        resources["HoverBrush"] = Brush(dark ? "#3A3A3D" : "#E8E5EC");
        foreach (Window window in Application.Current.Windows) ApplyTitleBar(window);
    }
    public static SolidColorBrush Brush(string value) => new((Color)ColorConverter.ConvertFromString(value));
    public static TextBlock Label(string text) => new() { Text = text, Margin = new Thickness(0, 9, 0, 3) };
    public static TextBlock Hint(string text)
    {
        var block = new TextBlock { Text = text, FontSize = 12, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 3, 0, 10) };
        block.SetResourceReference(TextBlock.ForegroundProperty, "MutedBrush");
        return block;
    }
    public static TextBlock Heading(string text) => new() { Text = text, FontSize = 24, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 0, 0, 10) };
    public static Button Button(string text, Action action, bool accent = false)
    {
        var button = new Button { Content = text, HorizontalContentAlignment = HorizontalAlignment.Center };
        if (accent) { button.Background = Brush("#087DFF"); button.Foreground = Brushes.White; button.BorderBrush = Brushes.Transparent; }
        button.Click += (_, _) => action();
        return button;
    }
    public static CheckBox Check(string text, bool value) => new() { Content = text, IsChecked = value };
    public static FrameworkElement Icon(string name, double size = 20) => VectorIcon.Create(name, size);
    public static Image BrandMark(double size = 32) => new() { Source = BrandImage, Width = size, Height = size };
    public static Button IconButton(string name, string label, Action action, bool accent = false)
    {
        var button = Button(label, action, accent); button.Content = Icon(name, 18); button.Width = button.Height = 34;
        button.Padding = new Thickness(0); button.ToolTip = label; AutomationProperties.SetName(button, label);
        if (!accent) { button.Background = Brushes.Transparent; button.BorderBrush = Brushes.Transparent; }
        return button;
    }
    public static Border Card(UIElement child, Thickness? padding = null)
    {
        var border = new Border { Child = child, Padding = padding ?? new Thickness(16), CornerRadius = new CornerRadius(22), BorderThickness = new Thickness(0) };
        border.SetResourceReference(Border.BackgroundProperty, "SurfaceBrush"); border.SetResourceReference(Border.BorderBrushProperty, "BorderBrush"); return border;
    }
    public static FrameworkElement WindowHeader(Window window)
    {
        var header = new Grid { Height = 48, Background = Brushes.Transparent, Margin = new Thickness(16, 0, 8, 0) };
        header.ColumnDefinitions.Add(new ColumnDefinition()); header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var caption = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        caption.Children.Add(BrandMark(22));
        caption.Children.Add(new TextBlock { Text = window.Title.Replace("cap · ", ""), FontSize = 13, FontWeight = FontWeights.Medium, Margin = new Thickness(9, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center });
        header.Children.Add(caption);
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        var minimize = IconButton("minimize", "最小化", () => window.WindowState = WindowState.Minimized);
        buttons.Children.Add(minimize);
        if (window.ResizeMode is ResizeMode.CanResize or ResizeMode.CanResizeWithGrip)
            buttons.Children.Add(IconButton("maximize", "最大化 / 还原", () => window.WindowState = window.WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized));
        buttons.Children.Add(IconButton("close", "关闭", window.Close));
        Grid.SetColumn(buttons, 1); header.Children.Add(buttons);
        header.MouseLeftButtonDown += (_, e) => { if (e.OriginalSource is not System.Windows.Controls.Button && e.ButtonState == MouseButtonState.Pressed) { try { window.DragMove(); } catch (InvalidOperationException) { System.Diagnostics.Trace.WriteLine("Window drag ended before capture."); } } };
        return header;
    }
    public static void PlaceNear(Window window, PixelRect region)
    {
        window.WindowStartupLocation = WindowStartupLocation.Manual;
        void Position()
        {
            var monitor = CaptureService.GetMonitors().OrderByDescending(m => { var overlap = m.Intersect(region); return (long)overlap.Width * overlap.Height; }).First();
            var workArea = System.Windows.Forms.Screen.FromPoint(new System.Drawing.Point(monitor.X + monitor.Width / 2, monitor.Y + monitor.Height / 2)).WorkingArea;
            monitor = new PixelRect(workArea.X, workArea.Y, workArea.Width, workArea.Height);
            nint handle = new WindowInteropHelper(window).Handle;
            var matrix = HwndSource.FromHwnd(handle)?.CompositionTarget?.TransformToDevice ?? Matrix.Identity;
            int width = (int)Math.Ceiling(window.ActualWidth * matrix.M11), height = (int)Math.Ceiling(window.ActualHeight * matrix.M22);
            int x = Math.Clamp(region.Right - width, monitor.X + 12, Math.Max(monitor.X + 12, monitor.Right - width - 12));
            int y = region.Bottom + 12;
            if (y + height > monitor.Bottom - 12) y = region.Y - height - 12;
            y = Math.Clamp(y, monitor.Y + 12, Math.Max(monitor.Y + 12, monitor.Bottom - height - 12));
            SetWindowPos(handle, 0, x, y, 0, 0, 0x15);
        }
        window.Loaded += (_, _) => Position();
        window.SizeChanged += (_, _) => { if (window.IsLoaded) Position(); };
    }
    public static TextBox Input(string text) => new() { Text = text };
    public static ComboBox Choice(params string[] entries)
    {
        var box = new ComboBox(); foreach (string entry in entries) box.Items.Add(entry); box.SelectedIndex = 0; return box;
    }
    public static void Error(Window? owner, Exception ex)
    {
        Cap.Platform.AppLog.Error(ex);
        if (owner != null && owner.IsVisible) MessageBox.Show(owner, ex.Message, "cap · 操作失败", MessageBoxButton.OK, MessageBoxImage.Warning);
        else MessageBox.Show(ex.Message, "cap · 操作失败", MessageBoxButton.OK, MessageBoxImage.Warning);
    }
    [DllImport("dwmapi.dll")] private static extern int DwmSetWindowAttribute(nint hwnd, int attribute, ref int value, int size);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint hwnd, nint after, int x, int y, int width, int height, uint flags);
}
