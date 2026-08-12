using System.Collections.ObjectModel;
using Avalonia.Media;
using Material.Icons;

namespace Abraxius.Lattice.ViewModels;

public sealed class ToolsViewModel : ViewModelBase
{
    private string _searchQuery = string.Empty;
    private ToolItemViewModel? _selectedTool;

    public string SearchQuery
    {
        get => _searchQuery;
        set
        {
            if (SetProperty(ref _searchQuery, value))
            {
                OnPropertyChanged(nameof(ResultCountText));
            }
        }
    }

    public string ResultCountText => Tools.Count == 0 ? "No tools" : $"{Tools.Count} tools";
    public ObservableCollection<ToolItemViewModel> Tools { get; } = new();

    public ToolItemViewModel? SelectedTool
    {
        get => _selectedTool;
        set => SetProperty(ref _selectedTool, value);
    }
}

public sealed class ToolItemViewModel : ViewModelBase
{
    public string DisplayName { get; init; } = string.Empty;
    public string ProviderName { get; init; } = string.Empty;
    public string CapabilityText { get; init; } = "Raw tool";
    public string AvailabilityText { get; init; } = "Unavailable";
    public string TrustText { get; init; } = "Unknown";
    public IBrush AvailabilityBrush { get; init; } = Brushes.Gray;
    public MaterialIconKind IconKind { get; init; } = IconLibrary.Tools;
}
