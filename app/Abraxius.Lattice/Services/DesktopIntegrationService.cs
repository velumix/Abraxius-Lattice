using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Threading;
using Abraxius.Lattice.Views;
using Abraxius.Lattice.ViewModels;

namespace Abraxius.Lattice.Services;

/// <summary>
/// Owns desktop-only lifecycle behavior: native tray presence and the policy
/// that a window close hides the UI instead of terminating the process.
/// </summary>
public sealed class DesktopIntegrationService : IDisposable
{
    private readonly IClassicDesktopStyleApplicationLifetime _lifetime;
    private readonly MainWindow _window;
    private readonly DaemonHostService _daemon = new();
    private TrayIcon? _trayIcon;
    private bool _exitRequested;
    private bool _disposed;

    public DesktopIntegrationService(IClassicDesktopStyleApplicationLifetime lifetime, MainWindow window)
    {
        _lifetime = lifetime;
        _window = window;
        _window.ViewModel.WorkspaceHome.WorkspaceSelected += OnWorkspaceSelected;
        _daemon.WorkspaceReady += OnWorkspaceReady;
    }

    public bool IsTrayAvailable => _trayIcon is not null;

    public void Initialize()
    {
        _window.Closing += OnWindowClosing;
        _window.Closed += OnWindowClosed;

        // The Studio plugin discovers this loopback bridge. Start the native
        // daemon with no workspace so the bridge is available immediately; a
        // later workspace-open flow can attach the authoritative project state.
        _daemon.Start(_window.ViewModel.WorkspaceHome.WorkspacePath);

        try
        {
            _trayIcon = new TrayIcon
            {
                Icon = IconFactory.CreateWindowIcon(),
                ToolTipText = "Abraxius Lattice",
                IsVisible = true,
                Menu = CreateTrayMenu(),
            };
            _trayIcon.Clicked += OnTrayIconClicked;
            TrayIcon.SetIcons(Application.Current!, new TrayIcons { _trayIcon });
        }
        catch (Exception exception) when (exception is PlatformNotSupportedException or InvalidOperationException)
        {
            _trayIcon = null;
            TraceUnavailable(exception);
        }

        // Refresh the launcher on every startup so a rebuilt/published app
        // cannot leave the desktop pointing at an older executable or icon.
        // A denied/unavailable desktop must not prevent the workstation from
        // launching.
        LogShortcutInstall(DesktopShortcutInstaller.Install());
    }

    public void ShowWindow()
    {
        if (_disposed)
        {
            return;
        }

        _window.Show();
        _window.WindowState = WindowState.Normal;
        _window.Activate();
    }

    public void Exit()
    {
        if (_disposed)
        {
            return;
        }

        _exitRequested = true;
        _lifetime.TryShutdown(0);
    }

    private NativeMenu CreateTrayMenu()
    {
        var menu = new NativeMenu();
        var show = new NativeMenuItem("Show Lattice");
        show.Click += (_, _) => ShowWindow();

        var install = new NativeMenuItem("Install desktop icon");
        install.Click += (_, _) => LogShortcutInstall(DesktopShortcutInstaller.Install());

        var exit = new NativeMenuItem("Exit Lattice");
        exit.Click += (_, _) => Exit();

        menu.Items.Add(show);
        menu.Items.Add(install);
        menu.Items.Add(new NativeMenuItemSeparator());
        menu.Items.Add(exit);
        return menu;
    }

    private void OnWindowClosing(object? sender, WindowClosingEventArgs e)
    {
        if (_exitRequested)
        {
            return;
        }

        if (_trayIcon is null)
        {
            // A platform without a native tray must not leave a headless
            // process behind. The close button remains a real close there.
            _exitRequested = true;
            _lifetime.TryShutdown(0);
            return;
        }

        e.Cancel = true;
        _window.Hide();
    }

    private void OnTrayIconClicked(object? sender, EventArgs e) => ShowWindow();

    private void OnWindowClosed(object? sender, EventArgs e) => Dispose();

    private void OnWorkspaceSelected(string workspacePath) => _daemon.OpenWorkspace(workspacePath);

    private void OnWorkspaceReady(DaemonWorkspaceStatus status) =>
        Dispatcher.UIThread.Post(() => _window.ViewModel.WorkspaceHome.ApplyWorkspaceReady(status));

    private static void TraceUnavailable(Exception exception) =>
        System.Diagnostics.Trace.TraceWarning("Lattice tray integration unavailable: {0}", exception.Message);

    private static void LogShortcutInstall(ShortcutInstallResult result) =>
        System.Diagnostics.Trace.TraceInformation(
            "Lattice desktop launcher refresh: {0} ({1}) {2}",
            result.Status,
            result.Location ?? "no location",
            result.Detail ?? "");

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _window.Closing -= OnWindowClosing;
        _window.Closed -= OnWindowClosed;
        _window.ViewModel.WorkspaceHome.WorkspaceSelected -= OnWorkspaceSelected;
        _daemon.WorkspaceReady -= OnWorkspaceReady;
        if (_window.DataContext is MainWindowViewModel viewModel)
        {
            viewModel.Dispose();
        }
        _daemon.Dispose();
        if (_trayIcon is not null)
        {
            _trayIcon.Clicked -= OnTrayIconClicked;
        }
        _trayIcon?.Dispose();
        _trayIcon = null;
    }
}
