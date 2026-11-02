using System;
using System.IO;
using System.Linq;
using System.Windows.Media.Imaging;
using Cap.Core;
using Cap.Recording;
using static Program;

internal static class EncoderTests
{
    internal static PixelFrame Solid(int width, int height, byte r, byte g, byte b)
    {
        byte[] data = new byte[width * height * 4];
        for (int p = 0; p < data.Length; p += 4) { data[p] = b; data[p + 1] = g; data[p + 2] = r; data[p + 3] = 255; }
        return new(width, height, data);
    }
    internal static GifBitmapDecoder DecodeGif(string path)
    {
        using var input = File.OpenRead(path);
        return new GifBitmapDecoder(input, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
    }
    internal static void GifRoundTrip()
    {
        string path = Path.Combine(DirectoryPath, "delta.gif");
        PixelFrame frame = Solid(160, 100, 0, 0, 0);
        var random = new Random(713);
        for (int p = 0; p < frame.Pixels.Length; p += 4)
        {
            frame.Pixels[p] = (byte)(random.Next(4) * 85);
            frame.Pixels[p + 1] = (byte)(random.Next(8) * 255 / 7);
            frame.Pixels[p + 2] = (byte)(random.Next(8) * 255 / 7);
        }
        using (var encoder = new GifEncoder(path, 160, 100))
        {
            encoder.Write(frame, 0, 333_333);
            encoder.Write(frame, 333_333, 333_333);
            var changed = frame with { Pixels = (byte[])frame.Pixels.Clone() };
            int p = (50 * 160 + 70) * 4;
            changed.Pixels[p] = 0; changed.Pixels[p + 1] = 0; changed.Pixels[p + 2] = 255;
            encoder.Write(changed, 666_666, 333_334);
            encoder.Write(changed, 1_000_000, 1_000_000);
        }
        var decoder = DecodeGif(path);
        Assert(decoder.Frames.Count == 2, "Identical frames must merge into two visible changes.");
        var first = decoder.Frames[0];
        byte[] indices = new byte[160 * 100];
        first.CopyPixels(indices, 160, 0);
        for (int p = 0; p < indices.Length; p++)
        {
            var color = first.Palette!.Colors[indices[p]];
            Assert(color.R == frame.Pixels[p * 4 + 2] && color.G == frame.Pixels[p * 4 + 1] && color.B == frame.Pixels[p * 4], $"LZW mismatch at pixel {p}: decoded {color}, expected RGB({frame.Pixels[p * 4 + 2]},{frame.Pixels[p * 4 + 1]},{frame.Pixels[p * 4]}).");
        }
        Assert((ushort)((BitmapMetadata)first.Metadata!).GetQuery("/grctlext/Delay") == 7, "First merged delay should be 7 centiseconds.");
        Assert(decoder.Frames[1].PixelWidth == 1 && decoder.Frames[1].PixelHeight == 1, "One changed pixel must encode as a 1x1 rectangle.");
        var metadata = (BitmapMetadata)decoder.Frames[1].Metadata!;
        Assert((ushort)metadata.GetQuery("/imgdesc/Left") == 70 && (ushort)metadata.GetQuery("/imgdesc/Top") == 50, "Delta offset must identify the changed pixel.");
        Assert((ushort)metadata.GetQuery("/grctlext/Delay") == 13, "Total GIF duration must be 20 centiseconds.");
    }
    internal static void GifTiming()
    {
        string path = Path.Combine(DirectoryPath, "timing.gif");
        using (var encoder = new GifEncoder(path, 2, 2))
            for (int i = 0; i < 60; i++)
                encoder.Write(Solid(2, 2, (byte)(i % 2 * 255), 0, 0), i * 10_000_000L / 60, (i + 1) * 10_000_000L / 60 - i * 10_000_000L / 60);
        var decoder = DecodeGif(path);
        int sum = decoder.Frames.Sum(frame => (ushort)((BitmapMetadata)frame.Metadata!).GetQuery("/grctlext/Delay"));
        Assert(sum == 100, "60 FPS GIF should encode exactly one second.");
    }
    internal static void Mp4RoundTrip()
    {
        string path = Path.Combine(DirectoryPath, "orientation.mp4");
        PixelFrame frame = Solid(160, 96, 255, 0, 0);
        for (int p = 160 * 48 * 4; p < frame.Pixels.Length; p += 4) { frame.Pixels[p] = 255; frame.Pixels[p + 2] = 0; }
        using (var encoder = new MfVideoEncoder(path, 160, 96, 30))
            for (int i = 0; i < 30; i++) encoder.Write(frame, i * 10_000_000L / 30, (i + 1) * 10_000_000L / 30 - i * 10_000_000L / 30);
        var decoded = Mp4Decoder.Decode(path);
        Assert(decoded.Width == 160 && decoded.Height == 96, "MP4 must have the requested dimensions.");
        Assert(decoded.Count == 30, "MP4 must contain all 30 decoded samples.");
        Assert(decoded.Subtype == MfNative.H264, "MP4 native stream must be H264.");
        Assert(Math.Abs(decoded.DurationTicks - 10_000_000L) < 100_000, "MP4 sample timestamps must span one second.");
        int top = (12 * 160 + 80) * 4, bottom = (80 * 160 + 80) * 4;
        Assert(decoded.First[top + 2] > 220 && decoded.First[top] < 35, "Top half should decode red, with no vertical inversion.");
        Assert(decoded.First[bottom] > 220 && decoded.First[bottom + 2] < 35, "Bottom half should decode blue.");
    }

    internal static void GifGradient()
    {
        string path = Path.Combine(DirectoryPath, "gradient.gif");
        var frame = Solid(256, 64, 0, 0, 0);
        for (int y = 0; y < 64; y++)
            for (int x = 0; x < 256; x++) frame.Pixels[(y * 256 + x) * 4] = (byte)x;
        using (var encoder = new GifEncoder(path, 256, 64)) encoder.Write(frame, 0, 1_000_000);
        var decoded = DecodeGif(path).Frames[0];
        var pixels = new byte[256 * 64];
        decoded.CopyPixels(pixels, 256, 0);
        for (int x = 4; x < 252; x += 8)
        {
            double average = 0;
            for (int y = 0; y < 64; y++)
                for (int dx = -4; dx < 4; dx++) average += decoded.Palette!.Colors[pixels[y * 256 + x + dx]].B;
            average /= 512;
            Assert(Math.Abs(average - (x - 0.5)) < 4, "Dithered local blue average should match the source gradient within 4 levels.");
        }
    }
}
