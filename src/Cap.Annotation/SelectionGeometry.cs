using System.Windows;
using Cap.Core;

namespace Cap.Annotation;

internal static class SelectionGeometry
{
    public static PixelRect Drag(Point from, Point to, int width, int height) =>
        PixelRect.FromPoints((int)Math.Clamp(from.X, 0, width), (int)Math.Clamp(from.Y, 0, height),
            (int)Math.Clamp(to.X, 0, width), (int)Math.Clamp(to.Y, 0, height));

    // Handles clockwise from top-left; -1 means inside, -2 means outside.
    public static int Hit(PixelRect rect, Point point, double tolerance)
    {
        if (rect.IsEmpty) return -2;
        Point[] handles = Handles(rect);
        for (int i = 0; i < handles.Length; i++)
            if (Math.Abs(handles[i].X - point.X) <= tolerance && Math.Abs(handles[i].Y - point.Y) <= tolerance) return i;
        return rect.Contains((int)point.X, (int)point.Y) ? -1 : -2;
    }

    public static Point[] Handles(PixelRect r) =>
    [new(r.X, r.Y), new(r.X + r.Width / 2d, r.Y), new(r.Right, r.Y), new(r.Right, r.Y + r.Height / 2d),
     new(r.Right, r.Bottom), new(r.X + r.Width / 2d, r.Bottom), new(r.X, r.Bottom), new(r.X, r.Y + r.Height / 2d)];

    public static PixelRect Adjust(PixelRect original, Point start, Point end, int handle, int width, int height)
    {
        int dx = (int)Math.Round(end.X - start.X), dy = (int)Math.Round(end.Y - start.Y);
        if (handle == -1)
            return original with { X = Math.Clamp(original.X + dx, 0, width - original.Width), Y = Math.Clamp(original.Y + dy, 0, height - original.Height) };
        int left = original.X, top = original.Y, right = original.Right, bottom = original.Bottom;
        if (handle is 0 or 6 or 7) left = Math.Clamp(left + dx, 0, right - 1);
        if (handle is 0 or 1 or 2) top = Math.Clamp(top + dy, 0, bottom - 1);
        if (handle is 2 or 3 or 4) right = Math.Clamp(right + dx, left + 1, width);
        if (handle is 4 or 5 or 6) bottom = Math.Clamp(bottom + dy, top + 1, height);
        return new(left, top, right - left, bottom - top);
    }
}
