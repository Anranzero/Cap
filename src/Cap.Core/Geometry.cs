namespace Cap.Core;

/// <summary>Physical desktop pixels. X/Y may be negative on secondary monitors.</summary>
public readonly record struct PixelRect(int X, int Y, int Width, int Height)
{
    public int Right => checked(X + Width);
    public int Bottom => checked(Y + Height);
    public bool IsEmpty => Width <= 0 || Height <= 0;
    public bool Contains(int x, int y) => x >= X && y >= Y && x < Right && y < Bottom;
    public PixelRect Intersect(PixelRect other)
    {
        int x = Math.Max(X, other.X), y = Math.Max(Y, other.Y);
        return new(x, y, Math.Max(0, Math.Min(Right, other.Right) - x), Math.Max(0, Math.Min(Bottom, other.Bottom) - y));
    }
    public static PixelRect FromPoints(int x1, int y1, int x2, int y2) => new(Math.Min(x1, x2), Math.Min(y1, y2), Math.Abs(x2 - x1), Math.Abs(y2 - y1));
}

/// <summary>Top-down BGRA32 pixels, tightly packed, opaque alpha.</summary>
public sealed record PixelFrame(int Width, int Height, byte[] Pixels)
{
    public int Stride => checked(Width * 4);
    public void Validate()
    {
        if (Width <= 0 || Height <= 0 || Pixels.Length != checked(Stride * Height))
            throw new ArgumentException("Invalid BGRA32 frame dimensions or buffer length.");
    }
}

public sealed record CaptureRequest(PixelRect Bounds, nint WindowHandle = 0, bool PreferGraphicsCapture = true);
public sealed record DesktopWindow(nint Handle, string Title, PixelRect Bounds);

public sealed class CaptureTargetClosedException : InvalidOperationException
{
    public CaptureTargetClosedException() : base("The selected capture window has closed.") { }
}

public interface ICaptureSource : IDisposable
{
    PixelRect Bounds { get; }
    string BackendName { get; }
    PixelFrame Capture();
}
