using System.Windows;
using System.Windows.Media;
using Cap.Core;

namespace Cap.Annotation;

internal sealed class EditorSurface(ScreenshotWindow owner) : FrameworkElement
{
    protected override void OnRender(DrawingContext dc)
    {
        base.OnRender(dc);
        if (ActualWidth <= 0 || ActualHeight <= 0) return;
        dc.PushTransform(new ScaleTransform(ActualWidth / owner.Desktop.Width, ActualHeight / owner.Desktop.Height));
        owner.Document.DrawCurrent(dc);
        PixelRect selected = owner.Selection.IsEmpty ? owner.Hover : owner.Selection;
        double width = owner.Desktop.Width, height = owner.Desktop.Height;
        Brush dim = new SolidColorBrush(Color.FromArgb(120, 0, 0, 0));
        if (selected.IsEmpty) dc.DrawRectangle(dim, null, new Rect(0, 0, width, height));
        else
        {
            dc.DrawRectangle(dim, null, new Rect(0, 0, width, selected.Y));
            dc.DrawRectangle(dim, null, new Rect(0, selected.Bottom, width, height - selected.Bottom));
            dc.DrawRectangle(dim, null, new Rect(0, selected.Y, selected.X, selected.Height));
            dc.DrawRectangle(dim, null, new Rect(selected.Right, selected.Y, width - selected.Right, selected.Height));
            if (owner.Scrolling) { dc.Pop(); return; } // The page shows through the cut-out; the scrolling outline draws the frame.
            dc.PushClip(new RectangleGeometry(new Rect(selected.X, selected.Y, selected.Width, selected.Height)));
            owner.Preview?.Draw(dc);
            dc.Pop();
            double physicalDip = owner.Desktop.Width / ActualWidth;
            // Feishu: recording regions use a pink frame, screenshots a #2596FF blue one.
            Color frame = owner.Mode == CaptureMode.Recording ? Color.FromRgb(0xDB, 0x5E, 0xA4) : Color.FromRgb(0x25, 0x96, 0xFF);
            var pen = new Pen(new SolidColorBrush(frame), 1.35 * physicalDip);
            dc.DrawRectangle(null, pen, new Rect(selected.X, selected.Y, selected.Width, selected.Height));
            if (!owner.Selection.IsEmpty && owner.Tool == AnnotationTool.Select)
            {
                // Feishu handles: filled blue discs with a thin white ring (~9dp diameter).
                var handleFill = new SolidColorBrush(frame);
                var ring = new Pen(Brushes.White, 1.4 * physicalDip);
                foreach (Point handle in SelectionGeometry.Handles(selected))
                    dc.DrawEllipse(handleFill, ring, handle, 4.6 * physicalDip, 4.6 * physicalDip);
            }
            string size = $"{selected.Width} × {selected.Height}";
            var text = PaintCommand.MakeText(size, 12 * physicalDip, Brushes.White);
            double labelY = Math.Max(0, selected.Y - text.Height - 8 * physicalDip);
            dc.DrawRoundedRectangle(new SolidColorBrush(Color.FromArgb(220, 25, 28, 34)), null,
                new Rect(selected.X, labelY, text.Width + 14 * physicalDip, text.Height + 6 * physicalDip), 4 * physicalDip, 4 * physicalDip);
            dc.DrawText(text, new Point(selected.X + 7 * physicalDip, labelY + 3 * physicalDip));
        }
        dc.Pop();
    }
}
