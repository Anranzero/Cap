using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using Cap.Visuals;

namespace Cap.Annotation;

/// <summary>
/// Feishu-style inline OCR panel: instead of closing the overlay and opening a
/// separate window, the recognized text appears inside the frozen-desktop overlay.
/// Measured from Feishu at 150% scaling: 280x350dp dark plate (#1A1A1A, radius 6dp),
/// bold 14dp title "提取文字", close glyph at the top-right and scrollable 13dp text.
/// </summary>
internal sealed class TextExtractPanel : Border
{
    private const double PanelWidth = 340;
    private const double PanelHeight = 400;

    private readonly TextBlock status;
    private readonly TextBox body;
    private readonly ProgressBar progress;
    private readonly bool dark;

    public event Action? CloseRequested;

    public TextExtractPanel(bool dark = true)
    {
        this.dark = dark;
        Width = PanelWidth;
        MaxHeight = PanelHeight;
        Background = new SolidColorBrush(dark ? Color.FromArgb(246, 26, 26, 26) : Color.FromRgb(0xFF, 0xFF, 0xFF));
        BorderBrush = new SolidColorBrush(dark ? Color.FromRgb(0x3C, 0x3C, 0x3C) : Color.FromRgb(0xE4, 0xE5, 0xE6));
        BorderThickness = new Thickness(1);
        CornerRadius = new CornerRadius(6);
        Effect = new System.Windows.Media.Effects.DropShadowEffect { Color = Colors.Black, BlurRadius = 16, ShadowDepth = 4, Opacity = .4 };

        var stack = new StackPanel();
        Child = stack;

        var header = new Grid { Margin = new Thickness(12, 10, 8, 4) };
        header.ColumnDefinitions.Add(new ColumnDefinition());
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var title = new TextBlock
        {
            Text = "提取文字", Foreground = dark ? Brushes.White : new SolidColorBrush(Color.FromRgb(0x1F, 0x23, 0x29)), FontSize = 14, FontWeight = FontWeights.Bold,
            VerticalAlignment = VerticalAlignment.Center
        };
        header.Children.Add(title);
        var close = new Button
        {
            Content = VectorIcon.Create("close", 12, new SolidColorBrush(Color.FromRgb(0xC8, 0xC8, 0xC8))),
            Width = 22, Height = 22, Background = Brushes.Transparent, BorderThickness = new Thickness(0),
            Padding = new Thickness(0), Cursor = Cursors.Hand, Focusable = false, ToolTip = "关闭",
            Template = AnnotationChrome.ButtonTemplate(true, 4)
        };
        AutomationProperties.SetName(close, "关闭提取文字");
        close.Click += (_, _) => CloseRequested?.Invoke();
        Grid.SetColumn(close, 1);
        header.Children.Add(close);
        stack.Children.Add(header);

        progress = new ProgressBar
        {
            Height = 2, Margin = new Thickness(12, 0, 12, 4), IsIndeterminate = true,
            Foreground = new SolidColorBrush(Color.FromRgb(0x4C, 0x88, 0xFF)),
            Background = new SolidColorBrush(Color.FromArgb(255, 45, 45, 45)),
            Visibility = Visibility.Collapsed
        };
        stack.Children.Add(progress);

        status = new TextBlock
        {
            Text = "正在识别文字…", Foreground = new SolidColorBrush(Color.FromRgb(0xC8, 0xC8, 0xC8)),
            FontSize = 13, Margin = new Thickness(12, 2, 12, 8), TextWrapping = TextWrapping.Wrap
        };
        stack.Children.Add(status);

        body = new TextBox
        {
            IsReadOnly = true, AcceptsReturn = true, AcceptsTab = true, TextWrapping = TextWrapping.Wrap,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
            Background = Brushes.Transparent, Foreground = dark ? Brushes.White : new SolidColorBrush(Color.FromRgb(0x1F, 0x23, 0x29)), BorderThickness = new Thickness(0),
            FontSize = 13, Padding = new Thickness(8, 0, 8, 8), MaxHeight = 250,
            Visibility = Visibility.Collapsed
        };
        stack.Children.Add(body);

        var actions = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 4, 4, 2) };
        var edit = BlueButton("编辑");
        edit.Click += (_, _) =>
        {
            if (!HasText) return;
            body.IsReadOnly = false;
            body.Focus();
            body.CaretIndex = body.Text.Length;
            status.Text = "可直接修改识别结果，改完点「复制」。";
        };
        var copy = BlueButton("复制");
        copy.Click += (_, _) => CopyAll();
        actions.Children.Add(edit);
        actions.Children.Add(copy);
        stack.Children.Add(actions);
        Visibility = Visibility.Collapsed;
    }

    /// <summary>Shows the busy state while OCR runs.</summary>
    public void ShowBusy()
    {
        status.Text = "正在识别文字…";
        status.Visibility = Visibility.Visible;
        progress.Visibility = Visibility.Visible;
        body.Visibility = Visibility.Collapsed;
        body.Text = "";
    }

    /// <summary>Shows the recognized text; an empty result keeps the explanatory hint.</summary>
    public void ShowText(string text)
    {
        progress.Visibility = Visibility.Collapsed;
        if (string.IsNullOrWhiteSpace(text))
        {
            status.Text = "未识别到文字，可重新框选清晰的文字区域。";
            status.Visibility = Visibility.Visible;
            body.Visibility = Visibility.Collapsed;
            return;
        }
        status.Text = "选择所需文字，或按 Ctrl+C 复制全部。";
        status.Visibility = Visibility.Visible;
        body.Text = text;
        body.Visibility = Visibility.Visible;
    }

    /// <summary>Shows a failure explanation inside the panel.</summary>
    public void ShowFailure(string message)
    {
        progress.Visibility = Visibility.Collapsed;
        status.Text = message;
        status.Visibility = Visibility.Visible;
        body.Visibility = Visibility.Collapsed;
    }

    /// <summary>Feishu-style blue action button used by the popup's 编辑 / 复制 row.</summary>
    private static Button BlueButton(string label)
    {
        var button = new Button
        {
            Content = new TextBlock { Text = label, Foreground = Brushes.White, FontSize = 12.5 },
            Height = 32, MinWidth = 64, Padding = new Thickness(14, 0, 14, 0), Margin = new Thickness(8, 0, 0, 0),
            Background = new SolidColorBrush(Color.FromRgb(0x33, 0x70, 0xEB)), BorderThickness = new Thickness(0),
            Cursor = Cursors.Hand, Focusable = false, Template = AnnotationChrome.ButtonTemplate(true, 6)
        };
        AutomationProperties.SetName(button, label);
        return button;
    }

    public bool HasText => body.Visibility == Visibility.Visible && !string.IsNullOrEmpty(body.Text);

    /// <summary>Copies every recognized line, matching the panel's hint.</summary>
    public bool CopyAll()
    {
        if (!HasText) return false;
        try
        {
            Clipboard.SetText(body.Text);
            return true;
        }
        catch (Exception error) when (error is System.Runtime.InteropServices.COMException or InvalidOperationException)
        {
            return false;
        }
    }
}
