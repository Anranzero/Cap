using System;
using System.Runtime.CompilerServices;
using Cap.Core;

[assembly: InternalsVisibleTo("Cap.Recording.Tests")]

namespace Cap.Recording;

internal interface IFrameEncoder : IDisposable
{
    void Write(PixelFrame frame, long time, long duration);
    void Complete();
}

internal static class FrameEncoder
{
    internal static IFrameEncoder Create(string path, RecordingFormat format, int width, int height, int fps) =>
        format == RecordingFormat.Gif ? new GifEncoder(path, width, height) : new MfVideoEncoder(path, width, height, fps);
}
