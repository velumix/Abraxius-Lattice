using Avalonia;
using Abraxius.Lattice.Services;

namespace Abraxius.Lattice;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        if (!SingleInstanceGuard.TryAcquire(out var singleInstance) || singleInstance is null)
        {
            return;
        }

        using (singleInstance)
        {
            BuildAvaloniaApp()
                .StartWithClassicDesktopLifetime(args);
        }
    }

    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
