namespace Cap.Core;

public enum RecordingFormat { Mp4, Gif }
public enum RecordingState { Idle, Recording, Paused, Stopping, Completed, Failed }

public sealed record RecordingOptions
{
    public required PixelRect Bounds { get; init; }
    public nint WindowHandle { get; init; }
    public RecordingFormat Format { get; init; } = RecordingFormat.Mp4;
    public int FramesPerSecond { get; init; } = 60;
    public int OutputWidth { get; init; }
    public int OutputHeight { get; init; }
    public bool ShowCursor { get; init; } = true;
    public bool HighlightClicks { get; init; } = true;
    public bool ShowCoordinates { get; init; } = true;
    public bool RelativeCoordinates { get; init; }
    public int CoordinateFontSize { get; init; } = 18;
    public string CoordinateColor { get; init; } = "#FFFFFF";
    public required string OutputPath { get; init; }
}

public sealed record RecordingProgress(TimeSpan Duration, long FrameCount, long DroppedFrames, string Backend);
public sealed record RecordingResult(string FilePath, TimeSpan Duration, long FrameCount);
