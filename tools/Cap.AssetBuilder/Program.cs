using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        if (args.Length != 2)
            throw new ArgumentException("Usage: Cap.AssetBuilder <output-directory> <master-png>");

        string directory = Path.GetFullPath(args[0]);
        string sourcePath = Path.GetFullPath(args[1]);
        var source = new BitmapImage();
        source.BeginInit();
        source.CacheOption = BitmapCacheOption.OnLoad;
        source.UriSource = new Uri(sourcePath);
        source.EndInit();
        source.Freeze();
        if (source.PixelWidth != source.PixelHeight || source.PixelWidth < 256)
            throw new InvalidDataException("The master icon must be square and at least 256 pixels wide.");

        Directory.CreateDirectory(directory);
        Int32Rect contentBounds = FindContentBounds(source);
        var artwork = new CroppedBitmap(source, contentBounds); artwork.Freeze();
        int[] sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
        var images = sizes.Select(size => Render(artwork, size)).ToArray();
        using (var writer = new BinaryWriter(File.Create(Path.Combine(directory, "cap.ico"))))
        {
            writer.Write((ushort)0); writer.Write((ushort)1); writer.Write((ushort)sizes.Length);
            uint offset = (uint)(6 + sizes.Length * 16);
            for (int i = 0; i < sizes.Length; i++)
            {
                writer.Write((byte)(sizes[i] == 256 ? 0 : sizes[i])); writer.Write((byte)(sizes[i] == 256 ? 0 : sizes[i]));
                writer.Write((byte)0); writer.Write((byte)0); writer.Write((ushort)1); writer.Write((ushort)32);
                writer.Write((uint)images[i].Length); writer.Write(offset); offset += (uint)images[i].Length;
            }
            foreach (byte[] image in images) writer.Write(image);
        }
        File.WriteAllBytes(Path.Combine(directory, "cap.png"), images[^1]);
        Console.WriteLine($"Artwork bounds: {contentBounds}; fitted to 96% of each icon canvas.");
        Console.WriteLine("Generated multi-resolution cap.ico and cap.png: " + directory);
    }
    private static Int32Rect FindContentBounds(BitmapSource source)
    {
        var bgra = new FormatConvertedBitmap(source, PixelFormats.Bgra32, null, 0);
        int stride = checked(source.PixelWidth * 4);
        byte[] pixels = new byte[checked(stride * source.PixelHeight)]; bgra.CopyPixels(pixels, stride, 0);
        int[] columns = new int[source.PixelWidth], rows = new int[source.PixelHeight];
        for (int y = 0; y < source.PixelHeight; y++)
            for (int x = 0; x < source.PixelWidth; x++)
                if (pixels[y * stride + x * 4 + 3] >= 128) { columns[x]++; rows[y]++; }
        // Ignore isolated translucent specks around the supplied artwork when fitting the icon.
        int columnMinimum = Math.Max(1, source.PixelHeight / 50), rowMinimum = Math.Max(1, source.PixelWidth / 50);
        int left = Array.FindIndex(columns, count => count >= columnMinimum), right = Array.FindLastIndex(columns, count => count >= columnMinimum);
        int top = Array.FindIndex(rows, count => count >= rowMinimum), bottom = Array.FindLastIndex(rows, count => count >= rowMinimum);
        if (left < 0 || top < 0) throw new InvalidDataException("The master icon has no visible artwork.");
        return new Int32Rect(left, top, right - left + 1, bottom - top + 1);
    }
    private static byte[] Render(BitmapSource source, int size)
    {
        var visual = new DrawingVisual();
        RenderOptions.SetBitmapScalingMode(visual, BitmapScalingMode.HighQuality);
        using (var dc = visual.RenderOpen())
        {
            double scale = size * .96 / Math.Max(source.PixelWidth, source.PixelHeight);
            double width = source.PixelWidth * scale, height = source.PixelHeight * scale;
            dc.DrawImage(source, new Rect((size - width) / 2, (size - height) / 2, width, height));
        }
        var bitmap = new RenderTargetBitmap(size, size, 96, 96, PixelFormats.Pbgra32); bitmap.Render(visual);
        using var memory = new MemoryStream(); var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap)); encoder.Save(memory); return memory.ToArray();
    }
}
