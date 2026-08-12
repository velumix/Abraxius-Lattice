using System.IO;
using Avalonia.Controls;

namespace Abraxius.Lattice.Services;

/// <summary>
/// Loads the user-provided Lattice artwork for native window/tray icons.
/// The PNG is wrapped in an ICO container so Windows, Avalonia and tray
/// implementations can consume the same source artwork without adding an
/// image-processing dependency to the workstation.
/// </summary>
internal static class IconFactory
{
    private const int Size = 32;

    public static WindowIcon CreateWindowIcon() =>
        new(new MemoryStream(CreateIcoBytes(), writable: false));

    public static void WriteIco(string path)
    {
        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var temporaryPath = path + ".tmp";
        File.WriteAllBytes(temporaryPath, CreateIcoBytes());
        File.Move(temporaryPath, path, overwrite: true);
    }

    private static byte[] CreateIcoBytes()
    {
        var pngPath = Path.Combine(AppContext.BaseDirectory, "Assets", "Lattice.png");
        if (File.Exists(pngPath))
        {
            var png = File.ReadAllBytes(pngPath);
            if (IsPng(png))
            {
                return WrapPngInIco(png);
            }
        }

        return CreateFallbackIcoBytes();
    }

    private static bool IsPng(byte[] bytes) =>
        bytes.Length >= 8
        && bytes[0] == 0x89
        && bytes[1] == 0x50
        && bytes[2] == 0x4E
        && bytes[3] == 0x47
        && bytes[4] == 0x0D
        && bytes[5] == 0x0A
        && bytes[6] == 0x1A
        && bytes[7] == 0x0A;

    private static byte[] WrapPngInIco(byte[] png)
    {
        // ICO permits a complete PNG payload as the image data. A zero width
        // and height in the directory entry means 256px or larger; the PNG
        // retains the original Lattice artwork and its alpha channel.
        const int headerSize = 6;
        const int directorySize = 16;
        using var stream = new MemoryStream(headerSize + directorySize + png.Length);
        using var writer = new BinaryWriter(stream);
        writer.Write((ushort)0);
        writer.Write((ushort)1);
        writer.Write((ushort)1);
        writer.Write((byte)0);
        writer.Write((byte)0);
        writer.Write((byte)0);
        writer.Write((byte)0);
        writer.Write((ushort)1);
        writer.Write((ushort)32);
        writer.Write(png.Length);
        writer.Write(headerSize + directorySize);
        writer.Write(png);
        return stream.ToArray();
    }

    private static byte[] CreateFallbackIcoBytes()
    {
        // One 32-bit 32x32 DIB image embedded in an ICO container. The pixels
        // are written bottom-up as required by the Windows icon format.
        const int headerSize = 6;
        const int directorySize = 16;
        const int dibHeaderSize = 40;
        const int pixelBytes = Size * Size * 4;
        const int maskBytes = Size * (Size / 8);
        var bytes = new byte[headerSize + directorySize + dibHeaderSize + pixelBytes + maskBytes];

        using var stream = new MemoryStream(bytes, writable: true);
        using var writer = new BinaryWriter(stream);
        writer.Write((ushort)0);
        writer.Write((ushort)1);
        writer.Write((ushort)1);

        writer.Write((byte)Size);
        writer.Write((byte)Size);
        writer.Write((byte)0);
        writer.Write((byte)0);
        writer.Write((ushort)1);
        writer.Write((ushort)32);
        writer.Write(pixelBytes + dibHeaderSize + maskBytes);
        writer.Write(headerSize + directorySize);

        writer.Write(dibHeaderSize);
        writer.Write(Size);
        writer.Write(Size * 2);
        writer.Write((ushort)1);
        writer.Write((ushort)32);
        writer.Write(0);
        writer.Write(pixelBytes);
        writer.Write(0);
        writer.Write(0);
        writer.Write(0);
        writer.Write(0);

        for (var y = Size - 1; y >= 0; y--)
        {
            for (var x = 0; x < Size; x++)
            {
                var distance = Math.Abs(x - 15.5) + Math.Abs(y - 15.5);
                var edge = distance is >= 12 and <= 15;
                var cyan = edge || (distance < 7 && (x + y) % 3 == 0);

                // BGRA order.
                writer.Write(cyan ? (byte)217 : (byte)28);
                writer.Write(cyan ? (byte)199 : (byte)24);
                writer.Write(cyan ? (byte)86 : (byte)21);
                writer.Write((byte)255);
            }
        }

        // Fully transparent AND mask: the 32-bit alpha channel is authoritative.
        writer.Write(new byte[maskBytes]);
        return bytes;
    }
}
