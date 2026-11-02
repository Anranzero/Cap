using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Effects;
using Cap.Visuals;

namespace Cap.Annotation;

internal sealed class TextPrompt : Window
{
    private readonly TextBox input;
    public string Value => input.Text.Trim();

    public TextPrompt(Window owner, bool dark)
    {
        Owner = owner;
        Title = "添加文字";
        Width = 410;
        Height = 286;
        ResizeMode = ResizeMode.NoResize;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        ShowInTaskbar = false;
        Topmost = true;
        WindowStartupLocation = WindowStartupLocation.Manual;
        Background = Brushes.Transparent;
        Foreground = AnnotationChrome.Foreground(dark);
        FontFamily = new FontFamily("Segoe UI, Microsoft YaHei UI");
        var frame = new Border
        {
            Background = AnnotationChrome.Surface(dark), BorderBrush = AnnotationChrome.Brush(dark ? "#39393D" : "#E6E2E8"),
            BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(12), Padding = new Thickness(18, 12, 18, 18), Margin = new Thickness(10),
            Effect = new DropShadowEffect { Color = Colors.Black, BlurRadius = 16, ShadowDepth = 3, Opacity = .18 }
        };
        var layout = new DockPanel();
        frame.Child = layout;
        Content = frame;
        var header = new DockPanel { Margin = new Thickness(0, 0, 0, 12), Background = Brushes.Transparent };
        DockPanel.SetDock(header, Dock.Top);
        Button close = AnnotationChrome.Button(VectorIcon.Create("close", 17, Foreground), "关闭添加文字", dark);
        close.IsCancel = true;
        close.Width = close.Height = 28;
        close.Margin = new Thickness(0);
        DockPanel.SetDock(close, Dock.Right);
        header.Children.Add(close);
        var title = new TextBlock { Text = "添加文字", FontSize = 16, FontWeight = FontWeights.SemiBold, VerticalAlignment = VerticalAlignment.Center };
        header.Children.Add(title);
        title.MouseLeftButtonDown += (_, _) => DragMove();
        layout.Children.Add(header);
        var footer = new DockPanel { Margin = new Thickness(0, 14, 0, 0) };
        DockPanel.SetDock(footer, Dock.Bottom);
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
        DockPanel.SetDock(buttons, Dock.Right);
        var cancel = AnnotationChrome.Button("取消", "取消", dark);
        cancel.IsCancel = true;
        cancel.Width = 64;
        cancel.FontSize = 13;
        cancel.Background = AnnotationChrome.Brush(dark ? "#39393D" : "#F0EDF2");
        cancel.Margin = new Thickness(0, 0, 8, 0);
        var confirmContent = new StackPanel { Orientation = Orientation.Horizontal };
        confirmContent.Children.Add(VectorIcon.Create("check", 16, Brushes.White));
        confirmContent.Children.Add(new TextBlock { Text = "添加", Foreground = Brushes.White, Margin = new Thickness(5, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center });
        var confirm = AnnotationChrome.Button(confirmContent, "添加文字 Ctrl+Enter", dark);
        confirm.Width = 76;
        confirm.Margin = new Thickness(0);
        confirm.Background = AnnotationChrome.Accent;
        confirm.Template = AnnotationChrome.ButtonTemplate(dark, accent: true);
        confirm.Click += (_, _) => { if (Value.Length > 0) DialogResult = true; };
        buttons.Children.Add(cancel);
        buttons.Children.Add(confirm);
        footer.Children.Add(buttons);
        footer.Children.Add(new TextBlock { Text = "Ctrl+Enter 添加", FontSize = 11, Foreground = AnnotationChrome.Brush(dark ? "#A5A6B0" : "#74747F"), VerticalAlignment = VerticalAlignment.Center });
        layout.Children.Add(footer);
        input = new TextBox
        {
            Style = new Style(typeof(TextBox)), AcceptsReturn = true, TextWrapping = TextWrapping.Wrap,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto, MaxLength = 4096, FontSize = 16,
            Background = Brushes.Transparent, Foreground = Foreground, CaretBrush = Foreground,
            BorderThickness = new Thickness(0), Padding = new Thickness(10), Margin = new Thickness(0)
        };
        System.Windows.Automation.AutomationProperties.SetName(input, "文字内容");
        input.PreviewKeyDown += (_, e) => { if (e.Key == Key.Enter && Keyboard.Modifiers.HasFlag(ModifierKeys.Control) && Value.Length > 0) { e.Handled = true; DialogResult = true; } };
        layout.Children.Add(new Border { Background = AnnotationChrome.Brush(dark ? "#131315" : "#FFFFFF"), BorderBrush = AnnotationChrome.Accent, BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(8), Child = input });
        Loaded += (_, _) =>
        {
            Point pointer = NativeDesktop.Cursor();
            Cap.Core.PixelRect monitor = NativeDesktop.MonitorAt(pointer);
            var dpi = VisualTreeHelper.GetDpi(this);
            int width = Math.Min(monitor.Width, (int)Math.Ceiling(Width * dpi.DpiScaleX));
            int height = Math.Min(monitor.Height, (int)Math.Ceiling(Height * dpi.DpiScaleY));
            NativeDesktop.Place(this, new Cap.Core.PixelRect(Math.Clamp((int)pointer.X, monitor.X, monitor.Right - width),
                Math.Clamp((int)pointer.Y, monitor.Y, monitor.Bottom - height), width, height));
            input.Focus();
        };
    }
}
