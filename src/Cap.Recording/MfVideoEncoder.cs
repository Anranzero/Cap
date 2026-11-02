using System;
using System.IO;
using System.Threading.Tasks;
using Cap.Core;

namespace Cap.Recording;

/// <summary>Windows inbox H.264 encoder and MPEG-4 sink. Input is explicitly top-down NV12.</summary>
internal sealed unsafe class MfVideoEncoder : IFrameEncoder
{
    private nint writer;
    private readonly int width, height;
    private readonly byte[] nv12;
    private uint stream;
    private bool startup, comInitialized, completed;

    internal MfVideoEncoder(string path, int width, int height, int fps)
    {
        if (width < 2 || height < 2 || (width & 1) != 0 || (height & 1) != 0)
            throw new ArgumentException("H.264 requires even output dimensions of at least 2 pixels.");
        if (File.Exists(path)) throw new IOException("Output file already exists.");
        this.width = width;
        this.height = height;
        nv12 = new byte[checked(width * height * 3 / 2)];
        try
        {
            int hr = MfNative.CoInitializeEx(0, 0);
            if (hr != unchecked((int)0x80010106)) { MfNative.Check(hr); comInitialized = true; }
            MfNative.Check(MfNative.MFStartup(0x20070, 0));
            startup = true;
            // Ask Media Foundation for a hardware H.264 MFT first (NVENC/QSV/AMF) and fall
            // back to the inbox software encoder automatically when none is available.
            nint attributes = 0;
            try
            {
                MfNative.Check(MfNative.MFCreateAttributes(out attributes, 1));
                MfNative.UInt32(attributes, MfNative.EnableHardwareTransforms, 1);
                MfNative.Check(MfNative.MFCreateSinkWriterFromURL(path, 0, attributes, out writer));
            }
            finally { MfNative.Release(ref attributes); }
            nint output = 0, input = 0;
            try
            {
                MfNative.Check(MfNative.MFCreateMediaType(out output));
                SetVideoType(output, MfNative.H264, width, height, fps);
                // Screen content compresses well; a lower bitrate keeps the encoder fast at 60 fps.
                uint bitrate = (uint)Math.Clamp((long)width * height * fps / 10, 600_000, 30_000_000);
                MfNative.UInt32(output, MfNative.Bitrate, bitrate);
                uint index;
                MfNative.Check(((delegate* unmanaged[Stdcall]<nint, nint, uint*, int>)MfNative.Slot(writer, 3))(writer, output, &index));
                stream = index;
                MfNative.Check(MfNative.MFCreateMediaType(out input));
                SetVideoType(input, MfNative.Nv12, width, height, fps);
                MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, nint, nint, int>)MfNative.Slot(writer, 4))(writer, stream, input, 0));
                MfNative.Invoke(writer, 5);
            }
            finally { MfNative.Release(ref input); MfNative.Release(ref output); }
        }
        catch (Exception error)
        {
            DisposeResources();
            throw new InvalidOperationException("Windows Media Foundation H.264 encoding could not start. On Windows N, install the Media Feature Pack. Verify the output dimensions and available disk space. " + error.Message, error);
        }
    }

    private static void SetVideoType(nint type, Guid subtype, int width, int height, int fps)
    {
        MfNative.GuidValue(type, MfNative.MajorType, MfNative.Video);
        MfNative.GuidValue(type, MfNative.Subtype, subtype);
        MfNative.UInt32(type, MfNative.Interlace, 2);
        MfNative.UInt64(type, MfNative.FrameSize, ((ulong)(uint)width << 32) | (uint)height);
        MfNative.UInt64(type, MfNative.FrameRate, ((ulong)(uint)fps << 32) | 1);
        MfNative.UInt64(type, MfNative.PixelAspect, (1UL << 32) | 1);
    }

    public void Write(PixelFrame frame, long time, long duration)
    {
        ObjectDisposedException.ThrowIf(writer == 0 || completed, this);
        frame.Validate();
        if (frame.Width != width || frame.Height != height || time < 0 || duration <= 0)
            throw new ArgumentException("Video frame dimensions or time are invalid.");
        ConvertNv12(frame.Pixels);
        nint sample = 0, buffer = 0;
        try
        {
            MfNative.Check(MfNative.MFCreateMemoryBuffer(nv12.Length, out buffer));
            nint data;
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, nint*, nint, nint, int>)MfNative.Slot(buffer, 3))(buffer, &data, 0, 0));
            try { System.Runtime.InteropServices.Marshal.Copy(nv12, 0, data, nv12.Length); }
            finally { MfNative.Invoke(buffer, 4); }
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, int, int>)MfNative.Slot(buffer, 6))(buffer, nv12.Length));
            MfNative.Check(MfNative.MFCreateSample(out sample));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, nint, int>)MfNative.Slot(sample, 42))(sample, buffer));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, long, int>)MfNative.Slot(sample, 36))(sample, time));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, long, int>)MfNative.Slot(sample, 38))(sample, duration));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, nint, int>)MfNative.Slot(writer, 6))(writer, stream, sample));
        }
        finally { MfNative.Release(ref sample); MfNative.Release(ref buffer); }
    }

    private void ConvertNv12(byte[] bgra)
    {
        int plane = width * height;
        // Parallel over row pairs: the previous serial loop could not keep up at 60 fps
        // on 1440p full-screen recordings.
        fixed (byte* source = bgra)
        fixed (byte* target = nv12)
        {
            byte* src = source;
            byte* dst = target;
            Parallel.For(0, height / 2, row =>
            {
                int y = row * 2;
                byte* top = src + (long)y * width * 4;
                byte* bottom = top + (long)width * 4;
                byte* lumaTop = dst + (long)y * width;
                byte* lumaBottom = lumaTop + width;
                byte* chroma = dst + plane + (long)row * width;
                for (int x = 0; x < width; x += 2)
                {
                    byte* t = top + x * 4;
                    byte* b = bottom + x * 4;
                    int b0 = t[0], g0 = t[1], r0 = t[2];
                    int b1 = t[4], g1 = t[5], r1 = t[6];
                    int b2 = b[0], g2 = b[1], r2 = b[2];
                    int b3 = b[4], g3 = b[5], r3 = b[6];
                    lumaTop[x] = Y(r0, g0, b0);
                    lumaTop[x + 1] = Y(r1, g1, b1);
                    lumaBottom[x] = Y(r2, g2, b2);
                    lumaBottom[x + 1] = Y(r3, g3, b3);
                    int rAvg = (r0 + r1 + r2 + r3) >> 2;
                    int gAvg = (g0 + g1 + g2 + g3) >> 2;
                    int bAvg = (b0 + b1 + b2 + b3) >> 2;
                    chroma[x] = Clamp(((-38 * rAvg - 74 * gAvg + 112 * bAvg + 128) >> 8) + 128);
                    chroma[x + 1] = Clamp(((112 * rAvg - 94 * gAvg - 18 * bAvg + 128) >> 8) + 128);
                }
            });
        }
    }

    private static byte Y(int r, int g, int b) =>
        Clamp(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
    private static byte Clamp(int value) => (byte)Math.Clamp(value, 0, 255);
    public void Complete()
    {
        if (completed || writer == 0) return;
        MfNative.Invoke(writer, 11);
        completed = true;
    }
    private void DisposeResources()
    {
        MfNative.Release(ref writer);
        if (startup) { MfNative.MFShutdown(); startup = false; }
        if (comInitialized) { MfNative.CoUninitialize(); comInitialized = false; }
    }
    public void Dispose()
    {
        try { Complete(); }
        finally { DisposeResources(); }
    }
}
