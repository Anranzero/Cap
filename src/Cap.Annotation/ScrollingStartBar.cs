using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using Cap.Visuals;

namespace Cap.Annotation;

/// <summary>
/// Feishu-style scrolling-capture starter: after selecting a region in scrolling
/// mode the overlay waits for an explicit start instead of scrolling immediately.
/// Measured from Feishu at 150% scaling: pill 258x45dp on #292929, radius 8dp,
/// with a play glyph plus "开始滚动截图" and a secondary "帮助" affordance.
/// </summary>
internal sealed class ScrollingStartBar : Border
{
    public event Action? StartRequested;
    public event Action? HelpRequested;

    public ScrollingStartBar()
    {
        Height = 45;
        CornerRadius = new CornerRadius(8);
        Background = AnnotationChrome.Brush("#292929");
        BorderBrush = AnnotationChrome.Brush("#3A3A3A");
        BorderThickness = new Thickness(1);
        Padding = new Thickness(10, 0, 10, 0);
        Effect = new System.Windows.Media.Effects.DropShadowEffect { Color = Colors.Black, BlurRadius = 14, ShadowDepth = 3, Opacity = .35 };

        var row = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        Child = row;

        var start = new Button
        {
            Content = BuildStart(), Background = Brushes.Transparent, BorderThickness = new Thickness(0),
            Padding = new Thickness(0), Cursor = Cursors.Hand, Focusable = false,
            ToolTip = "开始滚动截图", Template = AnnotationChrome.ButtonTemplate(true, 6)
        };
        start.Click += (_, _) => StartRequested?.Invoke();
        row.Children.Add(start);

        row.Children.Add(new Border
        {
            Width = 1, Height = 20, Background = AnnotationChrome.Brush("#3F3F3F"),
            Margin = new Thickness(10, 0, 10, 0), VerticalAlignment = VerticalAlignment.Center
        });

        var help = new Button
        {
            Content = BuildHelp(), Background = Brushes.Transparent, BorderThickness = new Thickness(0),
            Padding = new Thickness(0), Cursor = Cursors.Hand, Focusable = false,
            ToolTip = "帮助", Template = AnnotationChrome.ButtonTemplate(true, 6)
        };
        help.Click += (_, _) => HelpRequested?.Invoke();
        row.Children.Add(help);
    }

    private static FrameworkElement BuildStart()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(VectorIcon.Create("play", 13, Brushes.White));
        row.Children.Add(new TextBlock
        {
            Text = "开始滚动截图", Foreground = Brushes.White, FontSize = 13.5,
            Margin = new Thickness(7, 0, 4, 0), VerticalAlignment = VerticalAlignment.Center
        });
        return row;
    }

    private static FrameworkElement BuildHelp()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(VectorIcon.Create("info", 14, AnnotationChrome.Brush("#C8C8C8")));
        row.Children.Add(new TextBlock
        {
            Text = "帮助", Foreground = AnnotationChrome.Brush("#C8C8C8"), FontSize = 12.5,
            Margin = new Thickness(5, 0, 2, 0), VerticalAlignment = VerticalAlignment.Center
        });
        return row;
    }
}
