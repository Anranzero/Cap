using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using Cap.Core;
using Cap.Visuals;

namespace Cap.Annotation;

/// <summary>
/// Feishu-style recording starter shown inside the overlay after selecting a region
/// in recording mode: blue "开始录制" action, MP4/GIF format choice and a device row.
/// Measured from Feishu at 150% scaling: panel 320x196dp on #373737 with a #292929
/// device strip, 34dp blue action (#3370EB, radius 6dp), 12.5dp labels.
/// </summary>
internal sealed class RecordingStartPanel : Border
{
    private readonly Button mp4;
    private readonly Button gif;
    private RecordingFormat format = RecordingFormat.Mp4;

    public event Action<RecordingFormat>? StartRequested;

    public RecordingFormat Format => format;

    public RecordingStartPanel()
    {
        Width = 320;
        Background = new SolidColorBrush(Color.FromRgb(0x37, 0x37, 0x37));
        BorderBrush = new SolidColorBrush(Color.FromRgb(0x45, 0x45, 0x45));
        BorderThickness = new Thickness(1);
        CornerRadius = new CornerRadius(10);
        Padding = new Thickness(12, 12, 12, 10);
        Effect = new System.Windows.Media.Effects.DropShadowEffect { Color = Colors.Black, BlurRadius = 18, ShadowDepth = 4, Opacity = .45 };

        var stack = new StackPanel();
        Child = stack;

        var start = new Button
        {
            Content = new TextBlock { Text = "开始录制", Foreground = Brushes.White, FontSize = 14, FontWeight = FontWeights.SemiBold },
            Height = 34, Background = new SolidColorBrush(Color.FromRgb(0x33, 0x70, 0xEB)),
            BorderThickness = new Thickness(0), Cursor = Cursors.Hand, Focusable = false,
            ToolTip = "开始录制所选区域", Template = BlueTemplate()
        };
        AutomationProperties.SetName(start, "开始录制");
        start.Click += (_, _) => StartRequested?.Invoke(format);
        stack.Children.Add(start);

        var formatRow = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 10, 0, 0) };
        formatRow.Children.Add(new TextBlock
        {
            Text = "选择格式", Foreground = new SolidColorBrush(Color.FromRgb(0xC8, 0xC8, 0xC8)),
            FontSize = 12.5, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 14, 0)
        });
        mp4 = FormatOption("MP4", RecordingFormat.Mp4);
        gif = FormatOption("GIF", RecordingFormat.Gif);
        formatRow.Children.Add(mp4);
        formatRow.Children.Add(gif);
        stack.Children.Add(formatRow);

        var devices = new Border
        {
            Background = new SolidColorBrush(Color.FromRgb(0x29, 0x29, 0x29)),
            CornerRadius = new CornerRadius(6), Margin = new Thickness(0, 10, 0, 0), Padding = new Thickness(6, 6, 6, 6)
        };
        var row = new UniformGrid { Columns = 4 };
        row.Children.Add(Device("video", "扬声器", "cap 暂不支持音频采集", enabled: false));
        row.Children.Add(Device("settings", "麦克风", "cap 暂不支持音频采集", enabled: false));
        row.Children.Add(Device("monitor", "摄像头", "cap 暂不支持摄像头采集", enabled: false));
        row.Children.Add(Device("cursor", "鼠标设置", "录制时显示光标并高亮点击", enabled: true));
        devices.Child = row;
        stack.Children.Add(devices);

        UpdateFormat();
    }

    private Button FormatOption(string label, RecordingFormat value)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        var dot = new System.Windows.Shapes.Ellipse
        {
            Width = 12, Height = 12, StrokeThickness = 1.2,
            Stroke = new SolidColorBrush(Color.FromRgb(0x8A, 0x8A, 0x8A)), Margin = new Thickness(0, 0, 5, 0),
            VerticalAlignment = VerticalAlignment.Center
        };
        content.Children.Add(dot);
        content.Children.Add(new TextBlock { Text = label, Foreground = Brushes.White, FontSize = 12.5, VerticalAlignment = VerticalAlignment.Center });
        var button = new Button
        {
            Content = content, Height = 26, Padding = new Thickness(0), Margin = new Thickness(0, 0, 14, 0),
            Background = Brushes.Transparent, BorderThickness = new Thickness(0), Cursor = Cursors.Hand,
            Focusable = false, ToolTip = label, Template = AnnotationChrome.ButtonTemplate(true, 5)
        };
        AutomationProperties.SetName(button, label);
        button.Click += (_, _) => { format = value; UpdateFormat(); };
        return button;
    }

    private void UpdateFormat()
    {
        void Paint(Button button, bool selected)
        {
            if (((StackPanel)button.Content).Children[0] is System.Windows.Shapes.Ellipse dot)
            {
                dot.Fill = selected ? new SolidColorBrush(Color.FromRgb(0x33, 0x70, 0xEB)) : Brushes.Transparent;
                dot.Stroke = selected ? new SolidColorBrush(Color.FromRgb(0x33, 0x70, 0xEB)) : new SolidColorBrush(Color.FromRgb(0x8A, 0x8A, 0x8A));
            }
        }
        Paint(mp4, format == RecordingFormat.Mp4);
        Paint(gif, format == RecordingFormat.Gif);
    }

    private FrameworkElement Device(string icon, string label, string hint, bool enabled)
    {
        var content = new StackPanel { HorizontalAlignment = HorizontalAlignment.Center };
        var glyph = new Grid { Height = 18 };
        var iconElement = VectorIcon.Create(icon, 16, new SolidColorBrush(enabled ? Color.FromRgb(0xE8, 0xE8, 0xE8) : Color.FromRgb(0x7A, 0x7A, 0x7A)));
        glyph.Children.Add(iconElement);
        if (!enabled)
        {
            glyph.Children.Add(new System.Windows.Shapes.Line
            {
                X1 = 4, Y1 = 14, X2 = 18, Y2 = 2,
                Stroke = new SolidColorBrush(Color.FromRgb(0xC8, 0x3B, 0x3B)), StrokeThickness = 1.4
            });
        }
        content.Children.Add(glyph);
        content.Children.Add(new TextBlock
        {
            Text = label, FontSize = 11, Margin = new Thickness(0, 3, 0, 0),
            Foreground = new SolidColorBrush(enabled ? Color.FromRgb(0xE8, 0xE8, 0xE8) : Color.FromRgb(0x8A, 0x8A, 0x8A)),
            HorizontalAlignment = HorizontalAlignment.Center
        });
        var button = new Button
        {
            Content = content, Background = Brushes.Transparent, BorderThickness = new Thickness(0),
            Padding = new Thickness(0, 2, 0, 2), IsEnabled = enabled, Focusable = false,
            ToolTip = hint, Template = AnnotationChrome.ButtonTemplate(true, 5)
        };
        AutomationProperties.SetName(button, label);
        AutomationProperties.SetHelpText(button, hint);
        return button;
    }

    private static ControlTemplate BlueTemplate()
    {
        var border = new FrameworkElementFactory(typeof(Border), "Frame");
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(6));
        border.SetValue(Border.BackgroundProperty, new TemplateBindingExtension(Control.BackgroundProperty));
        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
        border.AppendChild(presenter);
        var template = new ControlTemplate(typeof(Button)) { VisualTree = border };
        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x2C, 0x63, 0xD2)), "Frame"));
        template.Triggers.Add(hover);
        var pressed = new Trigger { Property = System.Windows.Controls.Primitives.ButtonBase.IsPressedProperty, Value = true };
        pressed.Setters.Add(new Setter(UIElement.OpacityProperty, .8, "Frame"));
        template.Triggers.Add(pressed);
        return template;
    }
}
