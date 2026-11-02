using System;
using System.Collections.Generic;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using Cap.Core;
using Cap.Capture;
using Cap.Platform;

namespace Cap.App;

internal sealed class RecordingSetupWindow : Window
{
    private readonly AppSettings settings;
    private RecordingFormat format;
    private readonly ComboBox target, monitor, window, fps, coordinateMode;
    private readonly TextBox width, fontSize;
    private readonly ColorPickerButton color;
    private readonly CheckBox cursor, clicks, coordinates;
    private readonly IReadOnlyList<PixelRect> monitors;
    private readonly IReadOnlyList<DesktopWindow> windows;
    private readonly Button[] formatButtons = new Button[2];
    public bool SelectRegion => target.SelectedIndex == 0;
    public RecordingOptions? Options { get; private set; }

    public RecordingSetupWindow(AppSettings settings, RecordingFormat format)
    {
        Ui.PrepareWindow(this);
        this.settings = settings; this.format = format;
        Title = "cap · 录制";
        Width = 390; Height = 390; MinHeight = 360; ResizeMode = ResizeMode.CanMinimize; WindowStartupLocation = WindowStartupLocation.CenterScreen;
        monitors = CaptureService.GetMonitors(); windows = CaptureService.GetWindows().Where(w => w.Handle != new global::System.Windows.Interop.WindowInteropHelper(this).Handle && !w.Title.StartsWith("cap ·", StringComparison.Ordinal)).ToArray();
        target = Ui.Choice("已选区域", "显示器 / 全桌面", "指定窗口");
        monitor = Ui.Choice(monitors.Select((m, i) => $"显示器 {i + 1}  ·  {m.Width} × {m.Height}  ({m.X}, {m.Y})").Append("全部显示器").ToArray());
        window = Ui.Choice(windows.Count > 0 ? windows.Select(w => w.Title).ToArray() : new[] { "没有可录制窗口" });
        fps = format == RecordingFormat.Mp4 ? Ui.Choice("15", "24", "30", "60") : Ui.Choice("5", "10", "15", "24", "30", "60");
        if (format == RecordingFormat.Gif && !fps.Items.Contains(settings.GifFramesPerSecond.ToString())) fps.Items.Add(settings.GifFramesPerSecond.ToString());
        fps.SelectedItem = (format == RecordingFormat.Mp4 ? settings.FramesPerSecond : settings.GifFramesPerSecond).ToString();
        width = Ui.Input(format == RecordingFormat.Gif ? settings.GifMaxWidth.ToString() : "0");
        cursor = Ui.Check("录制鼠标指针", settings.ShowCursor); clicks = Ui.Check("点击高亮", settings.HighlightClicks);
        coordinates = Ui.Check("显示鼠标 X、Y 坐标", settings.ShowCoordinates);
        coordinateMode = Ui.Choice("屏幕绝对坐标", "录制区域相对坐标"); coordinateMode.SelectedIndex = settings.RelativeCoordinates ? 1 : 0;
        fontSize = Ui.Input(settings.CoordinateFontSize.ToString()); color = new ColorPickerButton(settings.CoordinateColor);

        var root = new Grid(); root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); root.RowDefinitions.Add(new RowDefinition()); root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.Children.Add(Ui.WindowHeader(this));
        var body = new StackPanel { Margin = new Thickness(20, 8, 20, 10) };
        body.Children.Add(FormatSelector());
        var regionHint = Ui.Hint("录制已选区域"); regionHint.Margin = new Thickness(0, 11, 0, 11); body.Children.Add(regionHint);
        body.Children.Add(SettingsPanel.Card(SettingsPanel.Switch("鼠标指针", null, cursor), SettingsPanel.Switch("显示鼠标坐标", null, coordinates)));

        var advanced = new StackPanel { Margin = new Thickness(0, 12, 0, 0) };
        advanced.Children.Add(SettingsPanel.Field("录制范围", target));
        var targetDetails = new Grid { Margin = new Thickness(0, 8, 0, 12) };
        monitor.Margin = window.Margin = new Thickness(0); monitor.MinHeight = window.MinHeight = 36;
        targetDetails.Children.Add(monitor); targetDetails.Children.Add(window); advanced.Children.Add(targetDetails);
        advanced.Children.Add(SettingsPanel.Pair(SettingsPanel.Field("帧率 · FPS", fps), SettingsPanel.Field("最大宽度 · px", width), 12));
        var widthHint = Ui.Hint("宽度填 0 保留原始尺寸。"); widthHint.Margin = new Thickness(0, 6, 0, 6); advanced.Children.Add(widthHint);
        advanced.Children.Add(SettingsPanel.Switch("点击高亮", null, clicks));
        var coordinateFields = new StackPanel { Margin = new Thickness(0, 8, 0, 6) };
        coordinateFields.Children.Add(SettingsPanel.Field("坐标方式", coordinateMode));
        var appearance = SettingsPanel.Pair(SettingsPanel.Field("字号 · 10–72", fontSize), SettingsPanel.Field("颜色", color), 12); appearance.Margin = new Thickness(0, 12, 0, 0); coordinateFields.Children.Add(appearance);
        coordinateFields.IsEnabled = coordinates.IsChecked == true;
        coordinates.Checked += (_, _) => coordinateFields.IsEnabled = true; coordinates.Unchecked += (_, _) => coordinateFields.IsEnabled = false;
        advanced.Children.Add(coordinateFields);
        var details = new Expander { Header = "高级设置", Content = advanced, IsExpanded = false, Margin = new Thickness(2, 5, 2, 0), FontSize = 12 };
        details.SetResourceReference(Control.ForegroundProperty, "MutedBrush");
        details.Expanded += (_, _) => Height = Math.Min(660, SystemParameters.WorkArea.Height - 40);
        details.Collapsed += (_, _) => Height = 390;
        body.Children.Add(details);
        var scroll = new ScrollViewer { Content = body, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled }; Grid.SetRow(scroll, 1); root.Children.Add(scroll);

        var start = Ui.Button("开始录制", Confirm, true); start.Margin = new Thickness(0); start.MinHeight = 40;
        root.Children.Add(Footer(start));
        void UpdateTarget()
        {
            monitor.Visibility = target.SelectedIndex == 1 ? Visibility.Visible : Visibility.Collapsed;
            window.Visibility = target.SelectedIndex == 2 ? Visibility.Visible : Visibility.Collapsed;
            targetDetails.Visibility = target.SelectedIndex == 0 ? Visibility.Collapsed : Visibility.Visible;
            regionHint.Text = target.SelectedIndex == 0 ? "录制已选区域" : target.SelectedIndex == 1 ? "录制显示器画面" : "录制指定窗口";
        }
        target.SelectionChanged += (_, _) => UpdateTarget(); UpdateTarget(); Content = root;
    }

    private FrameworkElement FormatSelector()
    {
        var row = new Grid();
        string[] labels = { "视频 MP4", "动图 GIF" };
        for (int index = 0; index < labels.Length; index++)
        {
            row.ColumnDefinitions.Add(new ColumnDefinition()); int selected = index;
            var button = Ui.Button(labels[index], () => ChangeFormat(selected == 0 ? RecordingFormat.Mp4 : RecordingFormat.Gif));
            button.Margin = new Thickness(index == 0 ? 0 : 4, 0, index == 0 ? 4 : 0, 0); button.Padding = new Thickness(10, 8, 10, 8); button.MinHeight = 36;
            System.Windows.Automation.AutomationProperties.SetName(button, labels[index]);
            formatButtons[index] = button; Grid.SetColumn(button, index); row.Children.Add(button);
        }
        UpdateFormatButtons();
        return row;
    }

    private void ChangeFormat(RecordingFormat selected)
    {
        if (format == selected) return;
        format = selected;
        fps.Items.Clear();
        foreach (string value in format == RecordingFormat.Mp4 ? new[] { "15", "24", "30", "60" } : new[] { "5", "10", "15", "24", "30", "60" }) fps.Items.Add(value);
        string defaultFps = (format == RecordingFormat.Mp4 ? settings.FramesPerSecond : settings.GifFramesPerSecond).ToString();
        if (!fps.Items.Contains(defaultFps)) fps.Items.Add(defaultFps);
        fps.SelectedItem = defaultFps; width.Text = format == RecordingFormat.Gif ? settings.GifMaxWidth.ToString() : "0";
        UpdateFormatButtons();
    }

    private void UpdateFormatButtons()
    {
        for (int index = 0; index < formatButtons.Length; index++)
        {
            bool selected = index == (format == RecordingFormat.Mp4 ? 0 : 1);
            formatButtons[index].SetResourceReference(Control.BackgroundProperty, selected ? "AccentSoftBrush" : "SurfaceBrush");
            formatButtons[index].SetResourceReference(Control.BorderBrushProperty, selected ? "AccentBrush" : "BorderBrush");
            formatButtons[index].SetResourceReference(Control.ForegroundProperty, selected ? "AccentBrush" : "MutedBrush");
        }
    }

    private FrameworkElement Footer(Button start)
    {
        var footer = new Border { Padding = new Thickness(20, 12, 20, 16), BorderThickness = new Thickness(0, 1, 0, 0) };
        footer.SetResourceReference(Border.BackgroundProperty, "SurfaceBrush"); footer.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        var content = new StackPanel();
        content.Children.Add(start);
        var location = Ui.Hint("保存到 " + settings.SaveDirectory); location.TextWrapping = TextWrapping.NoWrap; location.TextTrimming = TextTrimming.CharacterEllipsis; location.ToolTip = settings.SaveDirectory; location.Margin = new Thickness(0, 8, 0, 0); content.Children.Add(location);
        footer.Child = content; Grid.SetRow(footer, 2); return footer;
    }
    private void Confirm()
    {
        try
        {
            int maxWidth = int.Parse(width.Text); int size = int.Parse(fontSize.Text);
            if (maxWidth != 0 && maxWidth is < 64 or > 7680) throw new ArgumentException("宽度请选择 0 或 64–7680。");
            if (size is < 10 or > 72) throw new ArgumentException("坐标字号范围为 10–72。");
            PixelRect bounds = CaptureService.GetVirtualBounds(); nint handle = 0;
            if (target.SelectedIndex == 1 && monitor.SelectedIndex < monitors.Count) bounds = monitors[monitor.SelectedIndex];
            if (target.SelectedIndex == 2)
            {
                if (windows.Count == 0) throw new InvalidOperationException("没有可录制窗口。");
                var selected = windows[window.SelectedIndex]; bounds = selected.Bounds; handle = selected.Handle;
            }
            Options = new RecordingOptions
            {
                Bounds = bounds, WindowHandle = handle, Format = format, FramesPerSecond = int.Parse((string)fps.SelectedItem), OutputWidth = maxWidth,
                ShowCursor = cursor.IsChecked == true, HighlightClicks = clicks.IsChecked == true, ShowCoordinates = coordinates.IsChecked == true,
                RelativeCoordinates = coordinateMode.SelectedIndex == 1, CoordinateFontSize = size, CoordinateColor = color.Value, OutputPath = settings.SaveDirectory
            };
            DialogResult = true;
        }
        catch (Exception ex) { Ui.Error(this, ex); }
    }
}
