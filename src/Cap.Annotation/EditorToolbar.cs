using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Shapes;
using Cap.Visuals;

namespace Cap.Annotation;

/// <summary>
/// Feishu-style annotation chrome: a single primary tool row plus a contextual
/// options row (colour / stroke width / font size / effect strength) that appears
/// for the active drawing tool. Visual metrics mirror the Feishu screenshot
/// overlay captured at 150% display scale (primary row 40dp, options row 36dp,
/// surface #292929, active tool #3A4355, 18dp palette swatches).
/// </summary>
internal sealed class EditorToolbar : Border
{
    private const double RowHeight = 40;
    private const double OptionsHeight = 36;
    private const double ButtonSize = 34;
    private const double IconSize = 16;
    private const double Gap = 10;

    // Palette values measured separately per theme: Feishu shifts the swatch ramp in light mode.
    private static readonly string[] PaletteDark =
        ["#F05B56", "#FBCB46", "#51BA43", "#4C88FF", "#292929", "#757575", "#FFFFFF"];
    private static readonly string[] PaletteLight =
        ["#F54A45", "#FFC60A", "#35BD4B", "#336DF4", "#000000", "#8F959E", "#FFFFFF"];
    private string[] Palette => dark ? PaletteDark : PaletteLight;
    private static readonly string[] PaletteNames =
        ["红色", "黄色", "绿色", "蓝色", "深灰", "灰色", "白色"];
    private static readonly (string Label, double Value)[] Widths = [("细", 2), ("中", 3), ("粗", 6)];
    private static readonly (string Label, double Value)[] FontSizes =
        [("12pt", 2.4), ("16pt", 3.2), ("20pt", 4.0), ("24pt", 4.8), ("32pt", 6.4)];

    private readonly ScreenshotWindow owner;
    private readonly bool dark;
    private readonly bool regionOnly;
    private readonly Dictionary<AnnotationTool, Button> tools = [];
    private readonly Button? undo;
    private readonly Button? redo;
    private readonly StackPanel options = new() { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
    private readonly Border optionsBar;
    private readonly System.Windows.Shapes.Path optionsCaret;
    private string optionsSignature = "";
    private string fontLabel = "12pt";
    private Button? cancelButton;

    /// <summary>Raised when the grip is pressed; the window then owns the drag position.</summary>
    public event Action? DragStarted;
    public event Action? DragMoved;

    public EditorToolbar(ScreenshotWindow owner, bool dark, bool regionOnly)
    {
        this.owner = owner;
        this.dark = dark;
        this.regionOnly = regionOnly;
        Background = Brushes.Transparent;
        BorderThickness = new Thickness(0);
        Padding = new Thickness(0);
        UseLayoutRounding = true;

        var stack = new StackPanel { Orientation = Orientation.Vertical };
        Child = stack;

        Border primary = Bar(RowHeight, new Thickness(6, 5, 6, 5));
        var row = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        primary.Child = row;
        stack.Children.Add(primary);

        row.Children.Add(Grip());

        if (!regionOnly)
        {
            (AnnotationTool Tool, string Icon, string Hint)[] definitions =
            [
                (AnnotationTool.Rectangle, "rectangle", "矩形"),
                (AnnotationTool.Ellipse, "ellipse", "圆形"),
                (AnnotationTool.Line, "line", "直线"),
                (AnnotationTool.Arrow, "arrow", "箭头"),
                (AnnotationTool.Pen, "pen", "画笔"),
                (AnnotationTool.Text, "text", "文字"),
                (AnnotationTool.Number, "number", "序号")
            ];
            foreach (var item in definitions)
            {
                Button button = ToolButton(item.Icon, item.Hint, () => { owner.SetTool(item.Tool); Update(); });
                tools[item.Tool] = button;
                row.Children.Add(button);
            }
            row.Children.Add(Separator());
            foreach (var item in new[]
                     {
                         (AnnotationTool.Pixelate, "pixelate", "马赛克"),
                         (AnnotationTool.Blur, "blur", "模糊"),
                         (AnnotationTool.Marker, "marker", "标注")
                     })
            {
                Button button = ToolButton(item.Item2, item.Item3, () => { owner.SetTool(item.Item1); Update(); });
                tools[item.Item1] = button;
                row.Children.Add(button);
            }
            row.Children.Add(Separator());
            row.Children.Add(ToolButton("crop", "裁剪：拖动选区控制点调整范围", owner.BeginCrop));
            row.Children.Add(ToolButton("textScan", "提取文字：识别选区文字", owner.BeginOcr));
            row.Children.Add(ToolButton("autoScroll", "自动滚动：开始滚动截图", owner.BeginAutoScroll));
            row.Children.Add(ToolButton("copy", "复制到剪贴板 Ctrl+C", owner.Copy));
            row.Children.Add(Separator());
            undo = ToolButton("undo", "撤销 Ctrl+Z", owner.Undo);
            redo = ToolButton("redo", "重做 Ctrl+Y", owner.Redo);
            row.Children.Add(undo);
            row.Children.Add(redo);
            row.Children.Add(ToolButton("download", "保存 PNG / JPEG · Ctrl+S", owner.Save));
        }

        Button? cancel = CancelButton();
        row.Children.Add(cancel);
        row.Children.Add(ConfirmButton());

        optionsBar = Bar(OptionsHeight, new Thickness(8, 4, 8, 4));
        // Feishu renders the contextual row as a narrower pill aligned to the bar's left edge,
        // with a small caret on its top edge pointing at the active tool.
        optionsBar.HorizontalAlignment = HorizontalAlignment.Left;
        optionsBar.Child = options;
        optionsBar.Visibility = Visibility.Collapsed;
        optionsCaret = new System.Windows.Shapes.Path
        {
            Data = Geometry.Parse("M0 7L5 0L10 7Z"), Fill = AnnotationChrome.Brush(Surface),
            HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(26, Gap, 0, -1),
            Visibility = Visibility.Collapsed, IsHitTestVisible = false
        };
        var optionsHost = new StackPanel { Orientation = Orientation.Vertical, HorizontalAlignment = HorizontalAlignment.Left };
        optionsHost.Children.Add(optionsCaret);
        optionsHost.Children.Add(optionsBar);
        stack.Children.Add(optionsHost);

        Update();
    }

    private string Surface => dark ? "#292929" : "#FFFFFF";
    private string BorderColor => dark ? "#3A3A3A" : "#E4E4E7";
    private string IconBrush => dark ? "#E8E8E8" : "#2B2F36";
    private string ActiveBrush => dark ? "#3A4355" : "#D8DFEF";
    private string SeparatorBrush => dark ? "#3F3F3F" : "#CACBCD";
    private string MutedBrush => dark ? "#8A8A8A" : "#8F959E";
    /// <summary>Selected stroke width is marked with the accent blue in both themes.</summary>
    private string WidthAccent => dark ? "#4C88FF" : "#336DF4";
    /// <summary>Active tool icon turns accent blue on the light theme, stays near-white on dark.</summary>
    private string ActiveIconBrush => dark ? "#E8E8E8" : "#1456F0";

    private Border Bar(double height, Thickness padding) => new()
    {
        Height = height,
        Background = AnnotationChrome.Brush(Surface),
        BorderBrush = AnnotationChrome.Brush(BorderColor),
        BorderThickness = new Thickness(1),
        CornerRadius = new CornerRadius(10),
        Padding = padding,
        Effect = new DropShadowEffect { Color = Colors.Black, BlurRadius = 18, ShadowDepth = 4, Opacity = dark ? .38 : .14 }
    };

    private Border Grip()
    {
        var grip = new Border
        {
            Width = 22, Height = 28, Background = Brushes.Transparent, Cursor = Cursors.SizeAll,
            ToolTip = "拖动位置",
            Child = VectorIcon.Create("drag", 14, AnnotationChrome.Brush(MutedBrush))
        };
        AutomationProperties.SetName(grip, "拖动位置");
        grip.MouseLeftButtonDown += (_, e) => { if (grip.CaptureMouse()) DragStarted?.Invoke(); e.Handled = true; };
        grip.MouseMove += (_, e) =>
        {
            if (!grip.IsMouseCaptured) return;
            if (e.LeftButton == MouseButtonState.Pressed) DragMoved?.Invoke();
            else grip.ReleaseMouseCapture();
            e.Handled = true;
        };
        grip.MouseLeftButtonUp += (_, e) => { if (grip.IsMouseCaptured) grip.ReleaseMouseCapture(); e.Handled = true; };
        return grip;
    }

    private Border Separator() => new()
    {
        Width = 1, Height = 18, Background = AnnotationChrome.Brush(SeparatorBrush),
        Margin = new Thickness(Gap, 0, Gap, 0), VerticalAlignment = VerticalAlignment.Center
    };

    private Button ToolButton(string icon, string hint, Action action)
    {
        // Icon stroke follows the button foreground so the active tool can turn accent blue.
        Button button = AnnotationChrome.Button(VectorIcon.Create(icon, IconSize), hint, dark);
        button.Foreground = AnnotationChrome.Brush(IconBrush);
        button.Width = ButtonSize;
        button.Height = 28;
        button.Margin = new Thickness(1, 0, 1, 0);
        button.Template = AnnotationChrome.ButtonTemplate(dark, 6);
        button.Click += (_, _) => action();
        return button;
    }

    private Button CancelButton()
    {
        Button button = ToolButton("close", "取消 Esc", owner.Cancel);
        button.Content = VectorIcon.Create("close", 18, AnnotationChrome.Brush("#F05B56"));
        cancelButton = button;
        return button;
    }

    private Button ConfirmButton()
    {
        Button button = ToolButton("check", regionOnly ? "确认选区 Enter" : "复制完成：复制到剪贴板 Enter / Ctrl+C", owner.Complete);
        button.Content = VectorIcon.Create("check", 19, AnnotationChrome.Brush("#51BA43"));
        return button;
    }

    public void Update()
    {
        foreach (var pair in tools)
        {
            bool active = pair.Key == owner.Tool;
            pair.Value.Background = active ? AnnotationChrome.Brush(ActiveBrush) : Brushes.Transparent;
            pair.Value.Foreground = AnnotationChrome.Brush(active ? ActiveIconBrush : IconBrush);
        }
        if (undo is not null) undo.Opacity = owner.Document.CanUndo ? 1 : .35;
        if (redo is not null) redo.Visibility = owner.Document.CanRedo ? Visibility.Visible : Visibility.Collapsed;
        RebuildOptions();
    }

    /// <summary>Options row mirrors Feishu: width+colour for shapes, font size+colour for
    /// text and numbers, strength for effects. Hidden for the plain selection tool.</summary>
    private void RebuildOptions()
    {
        AnnotationTool tool = owner.Tool;
        bool visual = !regionOnly && tool != AnnotationTool.Select;
        if (!visual)
        {
            optionsBar.Visibility = Visibility.Collapsed;
            optionsCaret.Visibility = Visibility.Collapsed;
            optionsSignature = "";
            return;
        }
        optionsBar.Visibility = Visibility.Visible;
        optionsCaret.Visibility = Visibility.Visible;
        string signature = $"{tool}|{owner.InkColor}|{owner.InkThickness}|{fontLabel}|{owner.MarkerOpacity:F2}|{owner.TextStyle}";
        if (signature == optionsSignature) return;
        optionsSignature = signature;
        options.Children.Clear();

        if (tool is AnnotationTool.Text or AnnotationTool.Number)
        {
            if (tool == AnnotationTool.Text) options.Children.Add(TextStyleButtons());
            options.Children.Add(FontSizeMenu());
            options.Children.Add(Separator());
        }
        else if (tool == AnnotationTool.Marker)
        {
            options.Children.Add(OpacitySlider());
            options.Children.Add(Separator());
        }
        else if (tool is AnnotationTool.Pixelate or AnnotationTool.Blur)
        {
            options.Children.Add(EffectTypes());
            options.Children.Add(Separator());
            options.Children.Add(StrengthSlider());
            options.Children.Add(Separator());
        }
        else
        {
            options.Children.Add(WidthPicker());
            options.Children.Add(Separator());
        }

        for (int index = 0; index < Palette.Length; index++)
            options.Children.Add(Swatch(Palette[index], PaletteNames[index]));
    }

    private FrameworkElement CurrentColorDot()
    {
        var dot = new Border
        {
            Width = 14, Height = 14, CornerRadius = new CornerRadius(7),
            Background = new SolidColorBrush(owner.InkColor),
            BorderBrush = AnnotationChrome.Brush(dark ? "#6A6A6A" : "#C8C8CC"), BorderThickness = new Thickness(1),
            VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(2, 0, 8, 0), ToolTip = "当前颜色"
        };
        return dot;
    }

    private FrameworkElement WidthPicker()
    {
        var panel = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        foreach (var width in Widths)
        {
            bool active = Math.Abs(owner.InkThickness - width.Value) < .01;
            var dot = new Ellipse
            {
                Width = width.Value + 6, Height = width.Value + 6,
                Fill = AnnotationChrome.Brush(active ? WidthAccent : MutedBrush)
            };
            var button = new Button
            {
                Content = dot, Width = 26, Height = 26, Margin = new Thickness(1, 0, 1, 0),
                Background = Brushes.Transparent,
                BorderThickness = new Thickness(0), Padding = new Thickness(0), Focusable = false,
                Cursor = Cursors.Hand, ToolTip = width.Label, Template = AnnotationChrome.ButtonTemplate(dark, 6)
            };
            AutomationProperties.SetName(button, width.Label);
            button.Click += (_, _) => { owner.InkThickness = width.Value; Update(); };
            panel.Children.Add(button);
        }
        return panel;
    }

    /// <summary>Feishu text row: three lettering styles (plain / outlined / serif).</summary>
    private FrameworkElement TextStyleButtons()
    {
        var panel = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        for (int style = 0; style < 3; style++)
        {
            int captured = style;
            bool active = owner.TextStyle == style;
            var glyph = new Grid { Width = 18, Height = 18 };
            if (style == 1)
            {
                // Outlined lettering: black contour behind a white T, like Feishu's icon.
                glyph.Children.Add(new TextBlock
                {
                    Text = "T", FontFamily = new FontFamily("Microsoft YaHei UI"), FontWeight = FontWeights.Black,
                    FontSize = 17, Foreground = new SolidColorBrush(dark ? Colors.Black : Colors.White),
                    HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center
                });
                glyph.Children.Add(new TextBlock
                {
                    Text = "T", FontFamily = new FontFamily("Microsoft YaHei UI"), FontWeight = FontWeights.Black,
                    FontSize = 12.5, Foreground = AnnotationChrome.Brush(IconBrush),
                    HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center
                });
            }
            else
            {
                glyph.Children.Add(new TextBlock
                {
                    Text = "T", FontFamily = new FontFamily(style == 2 ? "SimSun" : "Microsoft YaHei UI"),
                    FontSize = 16, Foreground = AnnotationChrome.Brush(IconBrush),
                    HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center
                });
            }
            var button = new Button
            {
                Content = glyph, Width = 28, Height = 26, Margin = new Thickness(1, 0, 1, 0),
                Background = active ? AnnotationChrome.Brush(ActiveBrush) : Brushes.Transparent,
                BorderThickness = new Thickness(0), Padding = new Thickness(0), Focusable = false,
                Cursor = Cursors.Hand, ToolTip = style == 0 ? "普通文字" : style == 1 ? "描边文字" : "衬线文字",
                Template = AnnotationChrome.ButtonTemplate(dark, 6)
            };
            AutomationProperties.SetName(button, style == 0 ? "普通文字" : style == 1 ? "描边文字" : "衬线文字");
            button.Click += (_, _) => { owner.TextStyle = captured; Update(); };
            panel.Children.Add(button);
        }
        panel.Margin = new Thickness(0, 0, 6, 0);
        return panel;
    }

    private FrameworkElement FontSizeMenu()
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        content.Children.Add(new TextBlock
        {
            Text = fontLabel, FontSize = 12.5, Foreground = AnnotationChrome.Brush(IconBrush),
            VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 4, 0)
        });
        content.Children.Add(VectorIcon.Create("chevronDown", 11, AnnotationChrome.Brush(MutedBrush)));
        var menu = new ContextMenu
        {
            Background = AnnotationChrome.Brush(Surface), BorderBrush = AnnotationChrome.Brush(BorderColor),
            Foreground = AnnotationChrome.Brush(IconBrush), Padding = new Thickness(4)
        };
        foreach (var size in FontSizes)
        {
            var item = new MenuItem
            {
                Header = size.Label, IsCheckable = true, IsChecked = size.Label == fontLabel,
                Padding = new Thickness(10, 6, 14, 6), StaysOpenOnClick = false
            };
            item.Click += (_, _) =>
            {
                fontLabel = size.Label;
                owner.InkThickness = size.Value;
                menu.IsOpen = false;
                Update();
            };
            menu.Items.Add(item);
        }
        var button = new Button
        {
            Content = content, Width = 76, Height = 26, Padding = new Thickness(0), Focusable = false,
            Background = Brushes.Transparent, BorderThickness = new Thickness(0), Cursor = Cursors.Hand,
            ToolTip = "字号", Template = AnnotationChrome.ButtonTemplate(dark, 6), ContextMenu = menu
        };
        AutomationProperties.SetName(button, "字号");
        button.Click += (_, _) =>
        {
            menu.PlacementTarget = button;
            menu.Placement = PlacementMode.Bottom;
            menu.IsOpen = true;
        };
        owner.Closed += (_, _) => menu.IsOpen = false;
        return button;
    }

    private FrameworkElement EffectTypes()
    {
        var panel = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        (string Icon, string Hint)[] types = [("pixelate", "马赛克"), ("blur", "模糊"), ("rectangle", "块状")];
        for (int index = 0; index < types.Length; index++)
        {
            bool active = index == (owner.Tool == AnnotationTool.Pixelate ? 0 : 1);
            var button = new Button
            {
                Content = VectorIcon.Create(types[index].Icon, 16, AnnotationChrome.Brush(IconBrush)),
                Width = 28, Height = 26, Margin = new Thickness(1, 0, 1, 0), Focusable = false,
                Background = Brushes.Transparent,
                BorderThickness = new Thickness(0), Padding = new Thickness(0), Cursor = Cursors.Hand,
                ToolTip = types[index].Hint, Template = AnnotationChrome.ButtonTemplate(dark, 6)
            };
            int captured = index;
            button.Click += (_, _) =>
            {
                owner.SetTool(captured == 0 ? AnnotationTool.Pixelate : AnnotationTool.Blur);
                Update();
            };
            panel.Children.Add(button);
        }
        return panel;
    }

    /// <summary>Feishu's 标注 (highlighter) row: opacity slider defaulting to 30%.</summary>
    private FrameworkElement OpacitySlider()
    {
        var slider = new Slider
        {
            Width = 104, Minimum = 10, Maximum = 100, Value = Math.Clamp(owner.MarkerOpacity * 100, 10, 100),
            VerticalAlignment = VerticalAlignment.Center, IsMoveToPointEnabled = true,
            Foreground = AnnotationChrome.Brush(IconBrush), Background = Brushes.Transparent,
            SmallChange = 5, LargeChange = 10, Focusable = false
        };
        var label = new TextBlock
        {
            Text = $"{Math.Round(slider.Value)} %", FontSize = 12, Width = 38,
            Foreground = AnnotationChrome.Brush(IconBrush), VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(6, 0, 0, 0)
        };
        slider.ValueChanged += (_, e) =>
        {
            owner.MarkerOpacity = Math.Clamp(e.NewValue / 100, .1, 1);
            label.Text = $"{Math.Round(e.NewValue)} %";
        };
        var caption = new TextBlock
        {
            Text = "不透明度", FontSize = 12, Foreground = AnnotationChrome.Brush(MutedBrush),
            VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 6, 0)
        };
        var panel = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        panel.Children.Add(caption);
        panel.Children.Add(slider);
        panel.Children.Add(label);
        return panel;
    }

    private FrameworkElement StrengthSlider()
    {
        var slider = new Slider
        {
            Width = 104, Minimum = 1, Maximum = 12, Value = Math.Clamp(owner.InkThickness, 1, 12),
            VerticalAlignment = VerticalAlignment.Center, IsMoveToPointEnabled = true,
            Foreground = AnnotationChrome.Brush(IconBrush), Background = Brushes.Transparent,
            SmallChange = 1, LargeChange = 2, Focusable = false
        };
        var label = new TextBlock
        {
            Text = $"{Math.Round(slider.Value / 12 * 100)} %", FontSize = 12, Width = 38,
            Foreground = AnnotationChrome.Brush(IconBrush), VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(6, 0, 0, 0)
        };
        slider.ValueChanged += (_, e) =>
        {
            owner.InkThickness = e.NewValue;
            label.Text = $"{Math.Round(e.NewValue / 12 * 100)} %";
        };
        var caption = new TextBlock
        {
            Text = "强度", FontSize = 12, Foreground = AnnotationChrome.Brush(MutedBrush),
            VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 6, 0)
        };
        var panel = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        panel.Children.Add(caption);
        panel.Children.Add(slider);
        panel.Children.Add(label);
        return panel;
    }

    private Button Swatch(string value, string name)
    {
        var color = (Color)ColorConverter.ConvertFromString(value);
        bool active = owner.InkColor == color;
        var fill = new Border
        {
            Width = 18, Height = 18, CornerRadius = new CornerRadius(4),
            Background = new SolidColorBrush(color),
            BorderBrush = AnnotationChrome.Brush(dark ? "#4A4A4A" : "#D8D8DC"),
            BorderThickness = new Thickness(1)
        };
        var content = new Grid { Width = 22, Height = 22 };
        content.Children.Add(fill);
        if (active)
        {
            content.Children.Add(new Border
            {
                CornerRadius = new CornerRadius(6), BorderBrush = Brushes.White, BorderThickness = new Thickness(2),
                Visibility = dark ? Visibility.Visible : Visibility.Collapsed
            });
            content.Children.Add(VectorIcon.Create("check", 11, Brushes.White));
        }
        var button = new Button
        {
            Content = content, Width = 26, Height = 26, Margin = new Thickness(1, 0, 1, 0), Focusable = false,
            Background = Brushes.Transparent, BorderThickness = new Thickness(0), Padding = new Thickness(0),
            Cursor = Cursors.Hand, ToolTip = name, Template = AnnotationChrome.ButtonTemplate(dark, 5)
        };
        AutomationProperties.SetName(button, name);
        button.Click += (_, _) => { owner.InkColor = color; Update(); };
        return button;
    }

    private string InkHex => owner.InkColor.ToString();
}

internal static class AnnotationChrome
{
    internal static Brush Accent { get; } = Brush("#087DFF");
    internal static SolidColorBrush Brush(string value) => new((Color)ColorConverter.ConvertFromString(value));
    internal static Brush Foreground(bool dark) => Brush(dark ? "#F3F2F6" : "#202126");
    internal static Brush Surface(bool dark) => Brush(dark ? "#262627" : "#FCFAFC");

    internal static Button Button(object content, string hint, bool dark)
    {
        var button = new Button
        {
            Style = new Style(typeof(Button)), Content = content, ToolTip = hint,
            Foreground = Foreground(dark), Background = Brushes.Transparent, BorderBrush = Brushes.Transparent,
            // Feishu icon pitch is 40.3dp: 34dp buttons with 3dp side gutters.
            BorderThickness = new Thickness(0), Padding = new Thickness(0), Margin = new Thickness(3, 0, 3, 0),
            Width = 34, Height = 34, MinWidth = 0, MinHeight = 0,
            Cursor = System.Windows.Input.Cursors.Hand, Focusable = false, Template = ButtonTemplate(dark)
        };
        AutomationProperties.SetName(button, hint);
        return button;
    }

    internal static ControlTemplate ButtonTemplate(bool dark, double radius = 6, bool accent = false)
    {
        var border = new FrameworkElementFactory(typeof(Border), "Frame");
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(radius));
        border.SetValue(Border.BackgroundProperty, new TemplateBindingExtension(Control.BackgroundProperty));
        border.SetValue(Border.BorderBrushProperty, new TemplateBindingExtension(Control.BorderBrushProperty));
        border.SetValue(Border.BorderThicknessProperty, new TemplateBindingExtension(Control.BorderThicknessProperty));
        border.SetValue(Border.PaddingProperty, new TemplateBindingExtension(Control.PaddingProperty));
        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
        border.AppendChild(presenter);
        var template = new ControlTemplate(typeof(Button)) { VisualTree = border };
        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BackgroundProperty, Brush(accent ? "#006BDF" : dark ? "#353537" : "#F0EDF2"), "Frame"));
        template.Triggers.Add(hover);
        var pressed = new Trigger { Property = System.Windows.Controls.Primitives.ButtonBase.IsPressedProperty, Value = true };
        pressed.Setters.Add(new Setter(UIElement.OpacityProperty, .7, "Frame"));
        template.Triggers.Add(pressed);
        var disabled = new Trigger { Property = UIElement.IsEnabledProperty, Value = false };
        disabled.Setters.Add(new Setter(UIElement.OpacityProperty, .3, "Frame"));
        template.Triggers.Add(disabled);
        return template;
    }
}