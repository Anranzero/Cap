using System.Runtime.InteropServices;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX.Direct3D11;

namespace Cap.Capture;

/// <summary>Small, owned native D3D11 bridge. No third-party COM wrappers are required.</summary>
internal sealed unsafe class D3D11Bridge : IDisposable
{
    private static readonly Guid DxgiDeviceId = new("54ec77fa-1377-44e6-8c32-88fd5f44c84c");
    private static readonly Guid DxgiAccessId = new("a9b3d012-3df2-4ee3-b8d1-8695f457d3c1");
    private static readonly Guid Texture2DId = new("6f15aaf2-d208-4e89-9ab4-489535d34f9c");
    private nint device;
    private nint context;
    private nint staging;
    private int stagingWidth, stagingHeight;
    internal IDirect3DDevice WinRtDevice { get; }

    internal D3D11Bridge()
    {
        try
        {
            int result = D3D11CreateDevice(0, 1, 0, 0x20, 0, 0, 7, out device, out _, out context);
            if (result < 0)
            {
                Release(ref context);
                Release(ref device);
                result = D3D11CreateDevice(0, 5, 0, 0x20, 0, 0, 7, out device, out _, out context);
            }
            Marshal.ThrowExceptionForHR(result);
            nint dxgi = 0, inspectable = 0;
            try
            {
                Marshal.ThrowExceptionForHR(Marshal.QueryInterface(device, in DxgiDeviceId, out dxgi));
                Marshal.ThrowExceptionForHR(CreateDirect3D11DeviceFromDXGIDevice(dxgi, out inspectable));
                WinRtDevice = WinRT.MarshalInterface<IDirect3DDevice>.FromAbi(inspectable);
            }
            finally
            {
                Release(ref inspectable);
                Release(ref dxgi);
            }
        }
        catch
        {
            Dispose();
            throw;
        }
    }

    internal byte[]? ReadPixels(IDirect3DSurface surface, int width, int height, out int textureWidth, out int textureHeight, bool nonBlocking = false)
    {
        nint abi = 0, access = 0, texture = 0;
        try
        {
            abi = WinRT.MarshalInterface<IDirect3DSurface>.FromManaged(surface);
            Marshal.ThrowExceptionForHR(Marshal.QueryInterface(abi, in DxgiAccessId, out access));
            Guid id = Texture2DId;
            Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, Guid*, nint*, int>)Vtable(access)[3])(access, &id, &texture));
            TextureDescription description;
            ((delegate* unmanaged[Stdcall]<nint, TextureDescription*, void>)Vtable(texture)[10])(texture, &description);
            textureWidth = Math.Min(width, checked((int)description.Width));
            textureHeight = Math.Min(height, checked((int)description.Height));
            if (textureWidth <= 0 || textureHeight <= 0) throw new InvalidOperationException("The capture surface has no pixels.");
            EnsureStaging(description);
            ((delegate* unmanaged[Stdcall]<nint, nint, nint, void>)Vtable(context)[47])(context, staging, texture);
            MappedResource mapped;
            int hr = ((delegate* unmanaged[Stdcall]<nint, nint, uint, uint, uint, MappedResource*, int>)Vtable(context)[14])
                (context, staging, 0, 1, nonBlocking ? 0x10000u : 0u, &mapped);
            if (nonBlocking && hr != 0)
            {
                // DXGI_ERROR_WAS_STILL_DRAWING (0x887A0001) and S_FALSE both mean the copy is
                // not ready yet; the caller repeats the last frame and retries next tick.
                textureWidth = textureHeight = 0;
                return null;
            }
            Marshal.ThrowExceptionForHR(hr);
            try
            {
                int stride = checked(textureWidth * 4);
                byte[] pixels = GC.AllocateUninitializedArray<byte>(checked(stride * textureHeight));
                for (int row = 0; row < textureHeight; row++)
                    Marshal.Copy(mapped.Data + checked((nint)((long)row * mapped.RowPitch)), pixels, row * stride, stride);
                return pixels;
            }
            finally
            {
                ((delegate* unmanaged[Stdcall]<nint, nint, uint, void>)Vtable(context)[15])(context, staging, 0);
            }
        }
        finally
        {
            Release(ref texture);
            Release(ref access);
            Release(ref abi);
        }
    }

    private void EnsureStaging(TextureDescription source)
    {
        if (staging != 0 && stagingWidth == source.Width && stagingHeight == source.Height) return;
        Release(ref staging);
        var description = new TextureDescription
        {
            Width = source.Width,
            Height = source.Height,
            MipLevels = 1,
            ArraySize = 1,
            Format = 87,
            Sample = new() { Count = 1 },
            Usage = 3,
            CpuAccessFlags = 0x20000
        };
        nint texture = 0;
        Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, TextureDescription*, nint, nint*, int>)Vtable(device)[5])
            (device, &description, 0, &texture));
        staging = texture;
        stagingWidth = checked((int)source.Width);
        stagingHeight = checked((int)source.Height);
    }

    internal static GraphicsCaptureItem CreateItem(nint handle, bool window)
    {
        const string className = "Windows.Graphics.Capture.GraphicsCaptureItem";
        Guid interopId = new("3628e81b-3cac-4c60-b7f4-23ce0e0c3356");
        Guid itemId = new("79c3f95b-31f7-4ec2-a464-632ef5d30760");
        nint classString = 0, factory = 0, item = 0;
        int initialized = RoInitialize(1);
        // RPC_E_CHANGED_MODE means a WPF STA already initialized this thread.
        if (initialized < 0 && initialized != unchecked((int)0x80010106)) Marshal.ThrowExceptionForHR(initialized);
        try
        {
            Marshal.ThrowExceptionForHR(WindowsCreateString(className, className.Length, out classString));
            Marshal.ThrowExceptionForHR(RoGetActivationFactory(classString, in interopId, out factory));
            int index = window ? 3 : 4;
            Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, nint, Guid*, nint*, int>)Vtable(factory)[index])
                (factory, handle, &itemId, &item));
            return WinRT.MarshalInterface<GraphicsCaptureItem>.FromAbi(item);
        }
        finally
        {
            Release(ref item);
            Release(ref factory);
            if (classString != 0) WindowsDeleteString(classString);
            if (initialized >= 0) RoUninitialize();
        }
    }

    private static nint* Vtable(nint instance) => *(nint**)instance;
    private static void Release(ref nint instance)
    {
        nint owned = instance;
        instance = 0;
        if (owned != 0) Marshal.Release(owned);
    }

    public void Dispose()
    {
        if (WinRtDevice is IDisposable disposable) disposable.Dispose();
        Release(ref staging);
        Release(ref context);
        Release(ref device);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SampleDescription { internal uint Count, Quality; }
    [StructLayout(LayoutKind.Sequential)]
    private struct TextureDescription
    {
        internal uint Width, Height, MipLevels, ArraySize, Format;
        internal SampleDescription Sample;
        internal uint Usage, BindFlags, CpuAccessFlags, MiscFlags;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct MappedResource { internal nint Data; internal uint RowPitch, DepthPitch; }

    [DllImport("d3d11.dll")] private static extern int D3D11CreateDevice(nint adapter, uint driverType, nint software,
        uint flags, nint featureLevels, uint featureLevelCount, uint sdkVersion, out nint device, out uint featureLevel, out nint immediateContext);
    [DllImport("d3d11.dll")] private static extern int CreateDirect3D11DeviceFromDXGIDevice(nint dxgiDevice, out nint graphicsDevice);
    [DllImport("combase.dll")] private static extern int RoInitialize(uint initType);
    [DllImport("combase.dll")] private static extern void RoUninitialize();
    [DllImport("combase.dll", CharSet = CharSet.Unicode)] private static extern int WindowsCreateString(string source, int length, out nint value);
    [DllImport("combase.dll")] private static extern int WindowsDeleteString(nint value);
    [DllImport("combase.dll")] private static extern int RoGetActivationFactory(nint classId, in Guid iid, out nint factory);
}
