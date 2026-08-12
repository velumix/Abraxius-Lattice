using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.VisualTree;
using Abraxius.Lattice.ViewModels;

namespace Abraxius.Lattice.Views;

public partial class WorkspaceExplorerView : UserControl
{
    public WorkspaceExplorerView() => InitializeComponent();

    private void OnTreeItemExpanded(object? sender, RoutedEventArgs e)
    {
        if (e.Source is TreeViewItem { DataContext: ExplorerNodeViewModel node })
        {
            node.LoadChildren();
        }
    }

    private void OnTreeItemDoubleTapped(object? sender, RoutedEventArgs e)
    {
        if (e.Source is TreeViewItem { DataContext: ExplorerNodeViewModel node }
            && DataContext is WorkspaceExplorerViewModel explorer)
        {
            explorer.Activate(node);
            e.Handled = true;
        }
    }

    private void OnTreeNodePointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(null).Properties.PointerUpdateKind != PointerUpdateKind.LeftButtonPressed)
        {
            return;
        }

        if (e.Source is not Visual source
            || source.FindAncestorOfType<TreeViewItem>() is not { } item
            || item.DataContext is not ExplorerNodeViewModel node
            || DataContext is not WorkspaceExplorerViewModel explorer)
        {
            return;
        }

        // Make a row click authoritative instead of relying on TreeView's
        // container-selection timing. This keeps the editor and explorer in
        // sync even when the click lands on the icon/text presenter.
        explorer.SelectedNode = node;

        if (node.IsDirectory && !node.IsPlaceholder)
        {
            node.LoadChildren();
            item.IsExpanded = !item.IsExpanded;
        }

        e.Handled = true;
    }

    private void OnTreeSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        // SelectedItem is the authoritative two-way binding. Keeping this
        // event handler intentionally empty avoids opening a file twice when
        // Avalonia reports both a container and a data-item selection change.
    }
}
