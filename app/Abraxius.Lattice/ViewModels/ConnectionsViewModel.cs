using System.Collections.ObjectModel;
using Avalonia.Media;

namespace Abraxius.Lattice.ViewModels;

public sealed class ConnectionsViewModel : ViewModelBase
{
    private ProviderItemViewModel? _selectedProvider;

    public ObservableCollection<ProviderItemViewModel> Providers { get; } = new();

    public ProviderItemViewModel? SelectedProvider
    {
        get => _selectedProvider;
        set => SetProperty(ref _selectedProvider, value);
    }

    public RelayCommand AddProviderCommand { get; } = new(() => { });
}

public sealed class ProviderItemViewModel : ViewModelBase
{
    public string Name { get; init; } = string.Empty;
    public string TransportText { get; init; } = string.Empty;
    public string HealthText { get; init; } = "Unavailable";
    public string LatencyText { get; init; } = "—";
    public string ToolCountText { get; init; } = "—";
    public IBrush HealthBrush { get; init; } = Brushes.Gray;
}
