using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Abraxius.Lattice.Services;
using Abraxius.Lattice.Views;

namespace Abraxius.Lattice;

public partial class App : Application
{
    private DesktopIntegrationService? _desktopIntegration;

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // The tray owns process lifetime. Closing the main window is a
            // visibility action; only the tray's explicit Exit command shuts
            // down the UI process.
            desktop.ShutdownMode = ShutdownMode.OnExplicitShutdown;
            var mainWindow = new MainWindow();
            desktop.MainWindow = mainWindow;
            _desktopIntegration = new DesktopIntegrationService(desktop, mainWindow);
            _desktopIntegration.Initialize();
        }

        base.OnFrameworkInitializationCompleted();
    }
}
