namespace Cap.Core;

/// <summary>
/// Places the floating recording control bar. It must never overlap the Windows
/// taskbar: clamping to the full monitor let the bar sit on top of the taskbar,
/// where it flickered and clicks were swallowed by the taskbar.
/// </summary>
public static class RecordingBarPlacement
{
    public const int Margin = 8;

    /// <summary>Top-left corner (physical pixels) for a bar of <paramref name="width"/> x <paramref name="height"/>.</summary>
    public static (int X, int Y) Place(PixelRect bounds, PixelRect work, int width, int height)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(width);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(height);
        // Prefer below the recorded region, then above it; otherwise sit at the bottom of the
        // work area (just above the taskbar) centred on the recording, like Feishu does.
        int below = bounds.Bottom + Margin;
        int above = bounds.Y - height - Margin;
        int lowest = Math.Max(work.Y + Margin, work.Bottom - height - Margin);
        if (below + height > work.Bottom && above < work.Y)
        {
            int centred = bounds.X + (bounds.Width - width) / 2;
            int xl = Math.Clamp(centred, work.X, Math.Max(work.X, work.Right - width));
            return (xl, lowest);
        }
        int x = Math.Clamp(bounds.X, work.X, Math.Max(work.X, work.Right - width));
        int y = below + height <= work.Bottom ? below : above;
        y = Math.Clamp(y, work.Y + Margin, lowest);
        return (x, y);
    }
}
