using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cap.Core;

namespace Cap.Annotation;

/// <summary>
/// Feishu-style colour picker shown next to the cursor while the overlay is idle:
/// a magnified pixel sample with a centre marker plus a readout of the physical
/// coordinates, the sampled colour, and the copy / format shortcuts.
/// Measured from Feishu at 150% scaling: box 145x171dp, zoom pane 145x120dp,
/// readout lines 13dp white text on a #1A1A1A plate.
/// </summary>
internal sealed class ColorPickerPanel : Border
{
    private const double PaneWidth = 145;
    private const double PaneHeight = 120;
    private const double Zoom = 8;
    private const double Marker = 5;

    private readonly ScreenshotWindow owner;
    private readonly Image zoom = new() { Width = PaneWidth, Height = PaneHeight, Stretch = Stretch.Fill };
    private readonly Border marker;
    private readonly TextBlock coordinate;
    private readonly TextBlock value;
    private readonly Border swatch;
    private bool hex;
    private PixelRect sample;

    public ColorPickerPanel(ScreenshotWindow owner)
    {
        this.owner = owner;
        CornerRadius = new CornerRadius(3);
        BorderThickness = new Thickness(1);
        BorderBrush = AnnotationChrome.Brush("#4A4A4A");
        Background = AnnotationChrome.Brush("#1A1A1A");
        HorizontalAlignment = HorizontalAlignment.Left;
        VerticalAlignment = VerticalAlignment.Top;
        IsHitTestVisible = false;
        RenderOptions.SetBitmapScalingMode(zoom, BitmapScalingMode.NearestNeighbor);
        RenderOptions.SetEdgeMode(zoom, EdgeMode.Aliased);

        marker = new Border
        {
            Width = Marker, Height = Marker,
            BorderBrush = Brushes.White, BorderThickness = new Thickness(1),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Background = Brushes.Transparent
        };
        var pane = new Grid { Width = PaneWidth, Height = PaneHeight, ClipToBounds = true };
        pane.Children.Add(zoom);
        pane.Children.Add(marker);

        coordinate = new TextBlock
        {
            Foreground = Brushes.White, FontSize = 13, FontWeight = FontWeights.Bold,
            Margin = new Thickness(6, 5, 6, 1)
        };
        swatch = new Border
        {
            Width = 12, Height = 12, CornerRadius = new CornerRadius(2),
            BorderBrush = AnnotationChrome.Brush("#6A6A6A"), BorderThickness = new Thickness(1),
            VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(6, 0, 5, 0)
        };
        value = new TextBlock { Foreground = Brushes.White, FontSize = 13, VerticalAlignment = VerticalAlignment.Center };
        var valueRow = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 0, 6, 1) };
        valueRow.Children.Add(swatch);
        valueRow.Children.Add(value);
        var hints = new StackPanel { Margin = new Thickness(6, 1, 6, 6) };
        hints.Children.Add(new TextBlock
        {
            Text = "按 Ctrl + C 复制色值", Foreground = Brushes.White, FontSize = 12.5, Margin = new Thickness(0, 0, 0, 1)
        });
        hints.Children.Add(new TextBlock { Text = "按 Shift 切换 RGB/HEX", Foreground = Brushes.White, FontSize = 12.5 });

        var stack = new StackPanel();
        stack.Children.Add(pane);
        stack.Children.Add(coordinate);
        stack.Children.Add(valueRow);
        stack.Children.Add(hints);
        Child = stack;
        Visibility = Visibility.Collapsed;
    }

    /// <summary>Colour of the pixel under the pointer, in the frozen desktop frame.</summary>
    public Color Sampled { get; private set; }

    /// <summary>Pointer position in physical desktop coordinates.</summary>
    public PixelRect Sample => sample;

    public string ValueText => hex
        ? $"HEX: #{Sampled.R:X2}{Sampled.G:X2}{Sampled.B:X2}"
        : string.Create(CultureInfo.InvariantCulture, $"RGB:{Sampled.R},{Sampled.G},{Sampled.B}");

    /// <summary>Copies the value the readout currently shows, as Feishu does with Ctrl+C.</summary>
    public bool CopyValue()
    {
        if (Visibility != Visibility.Visible) return false;
        try
        {
            Clipboard.SetText(hex ? $"#{Sampled.R:X2}{Sampled.G:X2}{Sampled.B:X2}" : $"{Sampled.R},{Sampled.G},{Sampled.B}");
            return true;
        }
        catch (Exception error) when (error is System.Runtime.InteropServices.COMException or InvalidOperationException)
        {
            return false;
        }
    }

    /// <summary>Toggles RGB/HEX exactly like Shift does in Feishu.</summary>
    public void ToggleFormat()
    {
        hex = !hex;
        Update(sample, DesktopOrigin());
    }

    private Point DesktopOrigin() => new(owner.Desktop.X, owner.Desktop.Y);

    /// <summary>Refreshes the magnifier and readout for a pointer position (physical desktop coords).</summary>
    public void Update(PixelRect physical, Point origin)
    {
        Rect crop = new(physical.X - origin.X - 12, physical.Y - origin.Y - 10, PaneWidth / Zoom * (owner.Desktop.Width / Math.Max(1, owner.ActualWidth)), PaneHeight / Zoom * (owner.Desktop.Height / Math.Max(1, owner.ActualHeight)));
        sample = physical;
        BitmapSource background = owner.Document.Background;
        int left = (int)Math.Round(physical.X - origin.X - crop.Width / 2);
        int top = (int)Math.Round(physical.Y - origin.Y - crop.Height / 2);
        int width = Math.Max(2, (int)Math.Round(crop.Width));
        int height = Math.Max(2, (int)Math.Round(crop.Height));
        left = Math.Clamp(left, 0, Math.Max(0, background.PixelWidth - width));
        top = Math.Clamp(top, 0, Math.Max(0, background.PixelHeight - height));
        zoom.Source = new CroppedBitmap(background, new Int32Rect(left, top, width, height));
        Sampled = SampleAt(background, left + width / 2, top + height / 2);
        swatch.Background = new SolidColorBrush(Sampled);
        coordinate.Text = string.Create(CultureInfo.InvariantCulture, $"坐标: {physical.X},{physical.Y}");
        value.Text = ValueText;
    }

    private static Color SampleAt(BitmapSource source, int x, int y)
    {
        x = Math.Clamp(x, 0, source.PixelWidth - 1);
        y = Math.Clamp(y, 0, source.PixelHeight - 1);
        var pixel = new byte[4];
        source.CopyPixels(new Int32Rect(x, y, 1, 1), pixel, 4, 0);
        return Color.FromRgb(pixel[2], pixel[1], pixel[0]);
    }
}
