using System;
using System.Linq;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Shapes;

namespace Cap.App;

internal sealed class ColorPickerButton : Button
{
    private static readonly (string Name, string Hex)[] Presets =
    [
        ("白色", "#FFFFFF"), ("黑色", "#202126"), ("红色", "#F25F68"), ("橙色", "#FFAB52"),
        ("黄色", "#FFD85A"), ("绿色", "#45D69B"), ("蓝色", "#3388FF"), ("紫色", "#B58AF3")
    ];
    private readonly Border swatch = new() { Width = 18, Height = 18, CornerRadius = new CornerRadius(5), BorderThickness = new Thickness(1) };
    private readonly TextBlock label = new() { Margin = new Thickness(9, 0, 8, 0), VerticalAlignment = VerticalAlignment.Center };
    private readonly Border preview = new() { Height = 28, CornerRadius = new CornerRadius(7), BorderThickness = new Thickness(1), Margin = new Thickness(0, 4, 0, 8) };
    private readonly Popup popup;
    private readonly StackPanel panel;
    private readonly Expander more;
    private readonly Slider red, green, blue;
    private readonly Button firstPreset, done;
    private Window? owner;
    private bool updatingChannels;
    public string Value { get; private set; }

    public ColorPickerButton(string value)
    {
        Value = Normalize((Color)ColorConverter.ConvertFromString(value));
        SetResourceReference(StyleProperty, typeof(Button));
        SetResourceReference(BackgroundProperty, "InputBrush");
        HorizontalContentAlignment = HorizontalAlignment.Stretch; Padding = new Thickness(10, 7, 10, 7);
        ToolTip = "选择颜色";
        swatch.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        preview.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        var content = new DockPanel();
        var arrow = new Path { Data = Geometry.Parse("M0 0L4 4L8 0"), StrokeThickness = 1.4, Width = 8, Height = 4, VerticalAlignment = VerticalAlignment.Center };
        arrow.SetResourceReference(Shape.StrokeProperty, "MutedBrush");
        DockPanel.SetDock(arrow, Dock.Right); content.Children.Add(arrow);
        DockPanel.SetDock(swatch, Dock.Left); content.Children.Add(swatch); content.Children.Add(label); Content = content;

        panel = new StackPanel { Width = 224 };
        KeyboardNavigation.SetTabNavigation(panel, KeyboardNavigationMode.Cycle);
        var colors = new UniformGrid { Columns = 4, Margin = new Thickness(-3, 0, -3, 8) };
        Button? first = null;
        foreach (var preset in Presets)
        {
            var button = Ui.Button(preset.Name, () => { SelectColor(preset.Hex); ClosePopup(true); });
            button.Padding = new Thickness(4); button.Margin = new Thickness(3); button.MinHeight = 38;
            button.ToolTip = preset.Name; AutomationProperties.SetName(button, preset.Name);
            var tile = new Border { Width = 25, Height = 25, CornerRadius = new CornerRadius(8), Background = Ui.Brush(preset.Hex), BorderThickness = new Thickness(1) };
            tile.SetResourceReference(Border.BorderBrushProperty, "BorderBrush"); button.Content = tile;
            colors.Children.Add(button); first ??= button;
        }
        firstPreset = first!; panel.Children.Add(colors);
        var channels = new StackPanel(); channels.Children.Add(preview);
        red = Channel(channels, "红", "#F25F68");
        green = Channel(channels, "绿", "#45D69B");
        blue = Channel(channels, "蓝", "#3388FF");
        red.ValueChanged += ChannelChanged; green.ValueChanged += ChannelChanged; blue.ValueChanged += ChannelChanged;
        done = Ui.Button("使用此颜色", () => { SelectColor(Normalize(CurrentColor())); ClosePopup(true); }, true);
        done.Margin = new Thickness(0, 10, 0, 0); channels.Children.Add(done);
        more = new Expander { Header = "更多颜色", Content = channels, IsExpanded = false };
        panel.Children.Add(more);
        var frame = new Border { Child = panel, CornerRadius = new CornerRadius(14), BorderThickness = new Thickness(1), Padding = new Thickness(12), Margin = new Thickness(0, 4, 0, 4) };
        frame.SetResourceReference(Border.BackgroundProperty, "SurfaceBrush"); frame.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        frame.SetResourceReference(TextElement.ForegroundProperty, "TextBrush");
        popup = new Popup { Child = frame, PlacementTarget = this, Placement = PlacementMode.Bottom, StaysOpen = false, AllowsTransparency = true };
        popup.Opened += (_, _) => firstPreset.Focus();
        popup.Closed += (_, _) => { if (panel.IsKeyboardFocusWithin && IsVisible && owner?.IsActive == true) Focus(); };
        panel.PreviewKeyDown += PopupKeyDown;
        Loaded += (_, _) => AttachOwner();
        Unloaded += (_, _) => DetachOwner();
        IsEnabledChanged += (_, _) => { if (!IsEnabled) ClosePopup(false); };
        UpdateButton();
    }

    protected override void OnClick()
    {
        base.OnClick();
        if (popup.IsOpen) { ClosePopup(true); return; }
        Color color = (Color)ColorConverter.ConvertFromString(Value);
        updatingChannels = true;
        red.Value = color.R; green.Value = color.G; blue.Value = color.B;
        updatingChannels = false; preview.Background = new SolidColorBrush(color); more.IsExpanded = false;
        popup.IsOpen = true;
    }

    private static Slider Channel(Panel parent, string name, string accent)
    {
        var row = new DockPanel { Margin = new Thickness(0, 3, 0, 3) };
        row.Children.Add(new TextBlock { Text = name, Width = 26, VerticalAlignment = VerticalAlignment.Center });
        var slider = new Slider { Minimum = 0, Maximum = 255, TickFrequency = 1, IsSnapToTickEnabled = true, SmallChange = 1, LargeChange = 16, IsMoveToPointEnabled = true };
        slider.Resources["AccentBrush"] = Ui.Brush(accent); AutomationProperties.SetName(slider, name + "色分量");
        row.Children.Add(slider); parent.Children.Add(row); return slider;
    }

    private void ChannelChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (!updatingChannels) preview.Background = new SolidColorBrush(CurrentColor());
    }

    private Color CurrentColor() => Color.FromRgb((byte)red.Value, (byte)green.Value, (byte)blue.Value);
    private static string Normalize(Color color) => $"#{color.R:X2}{color.G:X2}{color.B:X2}";
    private void SelectColor(string value) { Value = value; UpdateButton(); }
    private void UpdateButton()
    {
        swatch.Background = Ui.Brush(Value);
        label.Text = Presets.FirstOrDefault(preset => preset.Hex == Value).Name ?? "自定义";
        AutomationProperties.SetName(this, "选择颜色：" + label.Text);
    }

    private void PopupKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape) { ClosePopup(true); e.Handled = true; return; }
        if (e.Key != Key.Tab || (Keyboard.Modifiers & ~ModifierKeys.Shift) != 0) return;
        bool backwards = Keyboard.Modifiers == ModifierKeys.Shift;
        if ((backwards && firstPreset.IsKeyboardFocused) || (!backwards && (done.IsKeyboardFocused || (!more.IsExpanded && more.IsKeyboardFocusWithin))))
        {
            ClosePopup(true);
            MoveFocus(new TraversalRequest(backwards ? FocusNavigationDirection.Previous : FocusNavigationDirection.Next));
            e.Handled = true;
        }
    }

    private void ClosePopup(bool restoreFocus)
    {
        popup.IsOpen = false;
        if (restoreFocus && IsVisible && IsEnabled) Focus();
    }
    private void AttachOwner()
    {
        DetachOwner(); owner = Window.GetWindow(this);
        if (owner == null) return;
        owner.Closed += OwnerClosed; owner.Deactivated += OwnerClosed; owner.IsVisibleChanged += OwnerVisibilityChanged;
    }
    private void DetachOwner()
    {
        ClosePopup(false);
        if (owner == null) return;
        owner.Closed -= OwnerClosed; owner.Deactivated -= OwnerClosed; owner.IsVisibleChanged -= OwnerVisibilityChanged; owner = null;
    }
    private void OwnerClosed(object? sender, EventArgs e) => ClosePopup(false);
    private void OwnerVisibilityChanged(object sender, DependencyPropertyChangedEventArgs e) { if (owner?.IsVisible != true) ClosePopup(false); }
}
