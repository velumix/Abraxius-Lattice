using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace Abraxius.Lattice.Controls;

public sealed class TimelineSurface : Control
{
    public static readonly StyledProperty<object?> TraceProperty =
        AvaloniaProperty.Register<TimelineSurface, object?>(nameof(Trace));

    public static readonly StyledProperty<object?> ViewportProperty =
        AvaloniaProperty.Register<TimelineSurface, object?>(nameof(Viewport));

    public static readonly StyledProperty<object?> TracksProperty =
        AvaloniaProperty.Register<TimelineSurface, object?>(nameof(Tracks));

    public static readonly StyledProperty<object?> SelectionProperty =
        AvaloniaProperty.Register<TimelineSurface, object?>(nameof(Selection));

    public static readonly StyledProperty<IBrush?> BackgroundProperty =
        AvaloniaProperty.Register<TimelineSurface, IBrush?>(nameof(Background));

    public static readonly StyledProperty<IBrush?> GridBrushProperty =
        AvaloniaProperty.Register<TimelineSurface, IBrush?>(nameof(GridBrush));

    public object? Trace
    {
        get => GetValue(TraceProperty);
        set => SetValue(TraceProperty, value);
    }

    public object? Viewport
    {
        get => GetValue(ViewportProperty);
        set => SetValue(ViewportProperty, value);
    }

    public object? Tracks
    {
        get => GetValue(TracksProperty);
        set => SetValue(TracksProperty, value);
    }

    public object? Selection
    {
        get => GetValue(SelectionProperty);
        set => SetValue(SelectionProperty, value);
    }

    public IBrush? Background
    {
        get => GetValue(BackgroundProperty);
        set => SetValue(BackgroundProperty, value);
    }

    public IBrush? GridBrush
    {
        get => GetValue(GridBrushProperty);
        set => SetValue(GridBrushProperty, value);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);
        context.FillRectangle(Background ?? Brushes.Transparent, new Rect(Bounds.Size));

        if (Trace is null)
        {
            return;
        }

        var pen = new Pen(GridBrush ?? Brushes.Gray, 1);
        var bounds = Bounds;
        const int divisions = 8;
        for (var index = 1; index < divisions; index++)
        {
            var x = bounds.Left + (bounds.Width * index / divisions);
            context.DrawLine(pen, new Point(x, bounds.Top), new Point(x, bounds.Bottom));
        }
    }
}
