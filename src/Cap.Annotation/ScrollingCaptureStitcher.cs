using Cap.Core;

namespace Cap.Annotation;

public enum ScrollAppendResult { Appended, Unchanged, NoReliableOverlap, LimitReached }

/// <summary>Conservative vertical stitching of overlapping, equally sized BGRA viewports.</summary>
public sealed class ScrollingCaptureStitcher
{
    private readonly List<byte[]> segments = [];
    private readonly int viewportHeight;
    private readonly long maximumPixels;
    private readonly int maximumHeight;
    private PixelFrame previous;

    public int Width { get; }
    public int Height { get; private set; }
    public bool CanUndo => segments.Count > 1;
    // Undo reconstructs the viewport from stitched pixels; compare the body, as alignment does,
    // because a sticky header appears only once in the output.
    internal bool MatchesCurrentViewport(PixelFrame frame) => frame.Width == Width && frame.Height == viewportHeight && Difference(previous, frame, 0, 45, 48) <= 3;

    public ScrollingCaptureStitcher(PixelFrame first, long maximumPixels = 40_000_000, int maximumHeight = 30_000)
    {
        first.Validate();
        if (maximumPixels <= 0 || maximumHeight <= 0 || (long)first.Width * first.Height > maximumPixels || first.Height > maximumHeight)
            throw new ArgumentOutOfRangeException(nameof(first), "所选区域超过长截图大小限制。");
        Width = first.Width;
        Height = viewportHeight = first.Height;
        this.maximumPixels = maximumPixels;
        this.maximumHeight = maximumHeight;
        previous = first with { Pixels = (byte[])first.Pixels.Clone() };
        segments.Add(previous.Pixels);
    }

    public ScrollAppendResult TryAppend(PixelFrame next)
    {
        next.Validate();
        if (next.Width != Width || next.Height != viewportHeight)
            throw new ArgumentException("长截图的每一屏必须具有相同尺寸。", nameof(next));
        if (AreSimilar(previous, next)) return ScrollAppendResult.Unchanged;

        int minimumShift = Math.Max(8, viewportHeight / 100);
        int maximumShift = viewportHeight - Math.Max(32, viewportHeight / 5);
        int bestShift = 0;
        double bestScore = double.MaxValue;
        var scores = new List<(int Shift, double Score)>();
        for (int shift = minimumShift; shift <= maximumShift; shift++)
        {
            double score = Difference(previous, next, shift, 9, 32);
            scores.Add((shift, score));
            if (score < bestScore) { bestScore = score; bestShift = shift; }
        }
        if (bestShift == 0 || bestScore > 3) return ScrollAppendResult.NoReliableOverlap;
        // Repeated bands, blank pages and reverse scrolling do not establish a unique forward offset.
        if (scores.Any(item => Math.Abs(item.Shift - bestShift) > 3 && item.Score < Math.Max(2, bestScore + 1.5)))
            return ScrollAppendResult.NoReliableOverlap;
        for (int shift = minimumShift; shift <= maximumShift; shift++)
            if (Difference(next, previous, shift, 9, 32) < Math.Max(2, bestScore + 1.5))
                return ScrollAppendResult.NoReliableOverlap;
        if (Difference(previous, next, bestShift, 45, 48) > 3 || HasStationaryFooter(previous, next))
            return ScrollAppendResult.NoReliableOverlap;
        if (Height + bestShift > maximumHeight || (long)Width * (Height + bestShift) > maximumPixels)
            return ScrollAppendResult.LimitReached;

        int length = checked(Width * bestShift * 4);
        var addition = new byte[length];
        Array.Copy(next.Pixels, next.Pixels.Length - length, addition, 0, length);
        segments.Add(addition);
        Height += bestShift;
        previous = next;
        return ScrollAppendResult.Appended;
    }

    public bool Undo()
    {
        if (!CanUndo) return false;
        Height -= segments[^1].Length / (Width * 4);
        segments.RemoveAt(segments.Count - 1);
        // Recover the preceding viewport from retained output, without retaining a full-frame history.
        int length = checked(Width * viewportHeight * 4);
        var pixels = new byte[length];
        int remaining = length;
        for (int i = segments.Count - 1; remaining > 0; i--)
        {
            byte[] segment = segments[i];
            int count = Math.Min(segment.Length, remaining);
            remaining -= count;
            Array.Copy(segment, segment.Length - count, pixels, remaining, count);
        }
        previous = new(Width, viewportHeight, pixels);
        return true;
    }

    /// <summary>Samples a grid to detect settled viewports; stitching performs a separate, denser alignment.</summary>
    public static bool AreSimilar(PixelFrame first, PixelFrame second)
    {
        if (first.Width != second.Width || first.Height != second.Height) return false;
        double total = 0;
        int count = 0;
        for (int y = 0; y < first.Height; y += Math.Max(1, first.Height / 48))
            for (int x = 0; x < first.Width; x += Math.Max(1, first.Width / 48))
            {
                int p = (y * first.Width + x) * 4;
                total += Math.Abs(Luminance(first.Pixels, p) - Luminance(second.Pixels, p));
                count++;
            }
        return total / count < .4;
    }

    private static double Difference(PixelFrame older, PixelFrame newer, int shift, int rows, int columns)
    {
        // Ignore a small header and horizontal edges (scrollbar / borders) when finding body movement.
        int top = Math.Max(2, older.Height / 10);
        // Include the previous viewport's very last row: a fixed footer must not be hidden at a seam.
        int bottom = older.Height - shift;
        if (bottom - top < 16) return double.MaxValue;
        double sum = 0;
        for (int r = 0; r < rows; r++)
        {
            int y = top + (bottom - top - 1) * r / (rows - 1);
            for (int c = 0; c < columns; c++)
            {
                int x = Math.Min(older.Width - 1, older.Width / 12 + (older.Width * 5 / 6 - 1) * c / (columns - 1));
                int a = ((y + shift) * older.Width + x) * 4;
                int b = (y * newer.Width + x) * 4;
                sum += Math.Abs(Luminance(older.Pixels, a) - Luminance(newer.Pixels, b));
            }
        }
        return sum / (rows * columns);
    }

    private static bool HasStationaryFooter(PixelFrame first, PixelFrame next)
    {
        int rows = Math.Max(3, Math.Min(12, first.Height / 20));
        double difference = 0, variation = 0;
        int count = 0;
        for (int y = first.Height - rows; y < first.Height; y++)
            for (int x = first.Width / 12; x < first.Width * 11 / 12; x += Math.Max(1, first.Width / 48))
            {
                int p = (y * first.Width + x) * 4;
                difference += Math.Abs(Luminance(first.Pixels, p) - Luminance(next.Pixels, p));
                if (x > first.Width / 12) variation += Math.Abs(Luminance(first.Pixels, p) - Luminance(first.Pixels, p - 4));
                count++;
            }
        return count > 0 && difference / count < 1 && variation / count > 3;
    }

    private static double Luminance(byte[] pixels, int offset) =>
        (pixels[offset] * 29 + pixels[offset + 1] * 150 + pixels[offset + 2] * 77) / 256d;

    public PixelFrame CreateFrame()
    {
        var pixels = new byte[checked(Width * Height * 4)];
        int offset = 0;
        foreach (byte[] segment in segments)
        {
            segment.CopyTo(pixels, offset);
            offset += segment.Length;
        }
        return new(Width, Height, pixels);
    }
}
