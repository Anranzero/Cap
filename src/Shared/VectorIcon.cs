using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Shapes;
using System.Windows.Data;

namespace Cap.Visuals;

/// <summary>Small consistent 24-unit stroke icons; no font or glyph dependency.</summary>
internal static class VectorIcon
{
    private static readonly IReadOnlyDictionary<string, string> Paths = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
    {
        ["capture"] = "M8 3H4A1 1 0 0 0 3 4V8 M16 3H20A1 1 0 0 1 21 4V8 M21 16V20A1 1 0 0 1 20 21H16 M8 21H4A1 1 0 0 1 3 20V16 M8 8H16V16H8Z",
        ["region"] = "M8 3H4A1 1 0 0 0 3 4V8 M16 3H20A1 1 0 0 1 21 4V8 M21 16V20A1 1 0 0 1 20 21H16 M8 21H4A1 1 0 0 1 3 20V16 M9 12H15 M12 9V15",
        ["video"] = "M4 6H14A2 2 0 0 1 16 8V16A2 2 0 0 1 14 18H4A2 2 0 0 1 2 16V8A2 2 0 0 1 4 6Z M16 10L22 6V18L16 14",
        ["gif"] = "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z M8 3V21 M16 3V21 M3 8H8 M3 16H8 M16 8H21 M16 16H21 M11 9L14 12L11 15Z",
        ["monitor"] = "M4 4H20A1 1 0 0 1 21 5V16A1 1 0 0 1 20 17H4A1 1 0 0 1 3 16V5A1 1 0 0 1 4 4Z M12 17V21 M8 21H16",
        ["window"] = "M4 4H20A1 1 0 0 1 21 5V19A1 1 0 0 1 20 20H4A1 1 0 0 1 3 19V5A1 1 0 0 1 4 4Z M3 9H21 M6 6.5H6.1 M9 6.5H9.1",
        ["select"] = "M5 3L18 13L12 14L9 20Z M12 14L16 21",
        ["cursor"] = "M5 3L18 13L12 14L9 20Z M12 14L16 21",
        ["rectangle"] = "M4 5H20V19H4Z",
        ["ellipse"] = "M21 12A9 7 0 1 1 3 12A9 7 0 1 1 21 12Z",
        ["arrow"] = "M5 19L19 5 M8 5H19V16",
        ["line"] = "M5 19L19 5",
        ["pen"] = "M4 17L16 5A2.1 2.1 0 0 1 19 8L7 20L3 21Z M14 7L17 10",
        ["text"] = "M4 7V4H20V7 M12 4V20 M8 20H16",
        ["number"] = "M12 3H20A1 1 0 0 1 21 4V12L11 22L2 13L12 3Z M16 8H16.1",
        ["pixelate"] = "M4 4H9V9H4Z M15 4H20V9H15Z M9.5 9.5H14.5V14.5H9.5Z M4 15H9V20H4Z M15 15H20V20H15Z",
        ["blur"] = "M4 6H5 M9 6H10 M14 6H15 M19 6H20 M6 12H7 M11 12H12 M16 12H17 M4 18H5 M9 18H10 M14 18H15 M19 18H20",
        ["undo"] = "M9 5L4 10L9 15 M4 10H14A6 6 0 0 1 14 22",
        ["redo"] = "M15 5L20 10L15 15 M20 10H10A6 6 0 0 0 10 22",
        ["pin"] = "M8 3H16 M9 3V8L6 13V15H18V13L15 8V3 M12 15V22",
        ["save"] = "M4 3H17L21 7V21H3V3Z M7 3V9H16V3 M7 21V14H17V21",
        ["download"] = "M12 3V15 M6 9L12 15L18 9 M4 20H20",
        ["crop"] = "M7 3V17H21 M3 7H17V21 M3 3L7 7 M21 21L17 17",
        ["marker"] = "M5 18L10 13 M8 21L13 16 M14 11L19 6 M11 8L16 3",
        ["autoScroll"] = "M5 5H9V9H5Z M15 5H19V9H15Z M5 15H9V19H5Z M15 15H19V19H15Z M12 5H14V7H12Z M12 15H14V17H12Z M5 12H7V14H5Z M15 12H17V14H15Z",
        ["textScan"] = "M4 8V5A1 1 0 0 1 5 4H8 M16 4H19A1 1 0 0 1 20 5V8 M20 16V19A1 1 0 0 1 19 20H16 M8 20H5A1 1 0 0 1 4 19V16 M8 9H16 M8 12H16 M8 15H13",
        ["copy"] = "M8 8H21V21H8Z M16 8V3H3V16H8",
        ["close"] = "M6 6L18 18 M6 18L18 6",
        ["check"] = "M4 12L9 17L20 6",
        ["chevronDown"] = "M6 9L12 15L18 9",
        ["chevronRight"] = "M9 6L15 12L9 18",
        ["arrowRight"] = "M4 12H20 M14 6L20 12L14 18",
        ["minus"] = "M5 12H19",
        ["minimize"] = "M5 12H19",
        ["plus"] = "M5 12H19 M12 5V19",
        ["play"] = "M8 4L20 12L8 20Z",
        ["pause"] = "M7 4V20 M17 4V20",
        ["stop"] = "M6 6H18V18H6Z",
        ["record"] = "M20 12A8 8 0 1 1 4 12A8 8 0 1 1 20 12Z",
        ["folder"] = "M3 7V5H9L12 8H21V20H3Z M3 11H21",
        ["keyboard"] = "M3 5H21V19H3Z M6 9H6.1 M10 9H10.1 M14 9H14.1 M18 9H18.1 M6 12H6.1 M10 12H10.1 M14 12H14.1 M18 12H18.1 M7 16H17",
        ["coordinates"] = "M12 2V22 M2 12H22 M9 5L12 2L15 5 M19 9L22 12L19 15 M8 16H16V8",
        ["settings"] = "M10 3H14L15 6L18 7L21 10V14L18 15L17 18L14 21H10L9 18L6 17L3 14V10L6 9L7 6Z M16 12A4 4 0 1 1 8 12A4 4 0 1 1 16 12Z",
        ["sun"] = "M16 12A4 4 0 1 1 8 12A4 4 0 1 1 16 12Z M12 2V4 M12 20V22 M2 12H4 M20 12H22 M5 5L6.5 6.5 M17.5 17.5L19 19 M5 19L6.5 17.5 M17.5 6.5L19 5",
        ["moon"] = "M20 15A9 9 0 0 1 9 4A9 9 0 1 0 20 15Z",
        ["info"] = "M21 12A9 9 0 1 1 3 12A9 9 0 1 1 21 12Z M12 11V17 M12 7H12.1",
        ["drag"] = "M8 5H8.1 M8 12H8.1 M8 19H8.1 M15 5H15.1 M15 12H15.1 M15 19H15.1",
        ["expand"] = "M8 3H3V8 M16 3H21V8 M3 16V21H8 M21 16V21H16",
        ["maximize"] = "M5 5H19V19H5Z",
        ["sparkle"] = "M12 3L14.5 9.5L21 12L14.5 14.5L12 21L9.5 14.5L3 12L9.5 9.5Z"
    };

    public static FrameworkElement Create(string name, double size = 20, Brush? foreground = null)
    {
        if (!Paths.TryGetValue(name, out string? data)) throw new ArgumentException("Unknown icon: " + name, nameof(name));
        var path = new Path { Data = Geometry.Parse(data), StrokeThickness = 1.7, StrokeStartLineCap = PenLineCap.Round, StrokeEndLineCap = PenLineCap.Round, StrokeLineJoin = PenLineJoin.Round };
        if (foreground == null) path.SetBinding(Shape.StrokeProperty, new Binding("Foreground") { RelativeSource = new RelativeSource(RelativeSourceMode.FindAncestor, typeof(Control), 1) }); else path.Stroke = foreground;
        var canvas = new Canvas { Width = 24, Height = 24 }; canvas.Children.Add(path);
        return new Viewbox { Width = size, Height = size, Child = canvas, IsHitTestVisible = false, SnapsToDevicePixels = true };
    }
}
