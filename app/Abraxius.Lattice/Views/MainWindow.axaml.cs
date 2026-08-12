using Avalonia.Controls;
using Avalonia.Platform.Storage;
using Abraxius.Lattice.Services;
using Abraxius.Lattice.ViewModels;

namespace Abraxius.Lattice.Views;

public partial class MainWindow : Window
{
    public MainWindowViewModel ViewModel { get; }

    public MainWindow()
    {
        InitializeComponent();
        Icon = IconFactory.CreateWindowIcon();
        ViewModel = new MainWindowViewModel();
        ViewModel.WorkspaceHome.FolderPicker = PickWorkspaceFolderAsync;
        DataContext = ViewModel;
    }

    private async Task<string?> PickWorkspaceFolderAsync()
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select a Lattice workspace",
            AllowMultiple = false,
        });

        return folders.FirstOrDefault()?.TryGetLocalPath();
    }
}
