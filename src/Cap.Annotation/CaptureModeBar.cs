using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using Cap.Visuals;

namespace Cap.Annotation;

internal sealed class CaptureModeBar : Border
{
    private readonly Dictionary<CaptureMode, Button> buttons = [];
    public CaptureMode SelectedMode { get; private set; }
    public event Action<CaptureMode>? Chosen;
    public event Action? DragStarted;
    public event Action? DragMoved;

    public CaptureModeBar(CaptureMode initialMode)
    {
        // Feishu's function bar: #0D0D0D plate, 12dp radius, 46dp tall, no visible outline.
        Background = new SolidColorBrush(Color.FromRgb(0x0D, 0x0D, 0x0D));
        CornerRadius = new CornerRadius(12); Padding = new Thickness(6, 8, 6, 8);
        BorderBrush = Brushes.Transparent; BorderThickness = new Thickness(0);
        var row = new WrapPanel(); Child = row;
        var grip = new Border
        {
            Width = 20, Height = 30, Background = Brushes.Transparent, Cursor = Cursors.SizeAll, ToolTip = "拖动位置",
            Child = VectorIcon.Create("drag", 13, new SolidColorBrush(Color.FromRgb(112, 112, 119)))
        };
        grip.MouseLeftButtonDown += (_, e) => { if (grip.CaptureMouse()) DragStarted?.Invoke(); e.Handled = true; };
        grip.MouseMove += (_, e) =>
        {
            if (!grip.IsMouseCaptured) return;
            if (e.LeftButton == MouseButtonState.Pressed) DragMoved?.Invoke();
            else grip.ReleaseMouseCapture();
            e.Handled = true;
        };
        grip.MouseLeftButtonUp += (_, e) => { if (grip.IsMouseCaptured) grip.ReleaseMouseCapture(); e.Handled = true; };
        row.Children.Add(grip);
        foreach (var entry in new[] { (CaptureMode.Screenshot, "截图"), (CaptureMode.Scrolling, "滚动截图"), (CaptureMode.Recording, "录屏"), (CaptureMode.Text, "提取文字") })
        {
            var mode = entry.Item1;
            var button = AnnotationChrome.Button(entry.Item2, entry.Item2, true);
            button.Width = mode is CaptureMode.Screenshot or CaptureMode.Recording ? 61 : 97;
            button.Height = 30; button.FontSize = 14; button.FontWeight = FontWeights.Medium; button.Margin = new Thickness(2, 0, 2, 0);
            button.Focusable = true;
            button.Template = AnnotationChrome.ButtonTemplate(true, 7);
            button.Click += (_, _) => { Select(mode); Chosen?.Invoke(mode); };
            buttons.Add(mode, button); row.Children.Add(button);
        }
        Select(initialMode);
    }
    public void Select(CaptureMode mode)
    {
        SelectedMode = mode;
        foreach (var pair in buttons)
        {
            pair.Value.Background = pair.Key == mode ? Brushes.White : Brushes.Transparent;
            pair.Value.Foreground = pair.Key == mode ? Brushes.Black : Brushes.WhiteSmoke;
        }
    }
    public void Move(int direction)
    {
        var modes = Enum.GetValues<CaptureMode>();
        int index = Array.IndexOf(modes, SelectedMode);
        Select(modes[(index + direction + modes.Length) % modes.Length]);
    }
}
