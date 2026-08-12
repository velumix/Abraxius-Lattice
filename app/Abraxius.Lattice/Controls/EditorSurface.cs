using System.Globalization;
using System.Text;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Input.Platform;
using Avalonia.Media;
using Avalonia.VisualTree;
using Abraxius.Lattice.Interop;
using Abraxius.Lattice.ViewModels;

namespace Abraxius.Lattice.Controls;

/// <summary>
/// A virtualized native editor surface.  Rust owns text, edits, selection and
/// history; this control only asks for visible lines and paints them.
/// </summary>
public sealed class EditorSurface : Control
{
    public static readonly StyledProperty<EditorViewModel?> SessionProperty =
        AvaloniaProperty.Register<EditorSurface, EditorViewModel?>(nameof(Session));

    public static readonly StyledProperty<IBrush?> BackgroundProperty =
        AvaloniaProperty.Register<EditorSurface, IBrush?>(nameof(Background));

    public static readonly StyledProperty<IBrush?> ForegroundProperty =
        AvaloniaProperty.Register<EditorSurface, IBrush?>(nameof(Foreground));

    public static readonly StyledProperty<IBrush?> GutterForegroundProperty =
        AvaloniaProperty.Register<EditorSurface, IBrush?>(nameof(GutterForeground));

    public static readonly StyledProperty<IBrush?> SelectionBrushProperty =
        AvaloniaProperty.Register<EditorSurface, IBrush?>(nameof(SelectionBrush));

    public static readonly StyledProperty<IBrush?> AccentBrushProperty =
        AvaloniaProperty.Register<EditorSurface, IBrush?>(nameof(AccentBrush));

    public static readonly StyledProperty<double> FontSizeProperty =
        AvaloniaProperty.Register<EditorSurface, double>(nameof(FontSize), 14);

    public static readonly StyledProperty<FontFamily> EditorFontFamilyProperty =
        AvaloniaProperty.Register<EditorSurface, FontFamily>(nameof(EditorFontFamily), FontFamily.Default);

    private const double GutterWidth = 64;
    private const double HorizontalPadding = 10;
    private double _verticalOffset;
    private double _horizontalOffset;
    private EditorViewModel? _observedSession;

    public EditorSurface()
    {
        Focusable = true;
        ClipToBounds = true;
    }

    public EditorViewModel? Session
    {
        get => GetValue(SessionProperty);
        set => SetValue(SessionProperty, value);
    }

    public IBrush? Background
    {
        get => GetValue(BackgroundProperty);
        set => SetValue(BackgroundProperty, value);
    }

    public IBrush? Foreground
    {
        get => GetValue(ForegroundProperty);
        set => SetValue(ForegroundProperty, value);
    }

    public IBrush? GutterForeground
    {
        get => GetValue(GutterForegroundProperty);
        set => SetValue(GutterForegroundProperty, value);
    }

    public IBrush? SelectionBrush
    {
        get => GetValue(SelectionBrushProperty);
        set => SetValue(SelectionBrushProperty, value);
    }

    public IBrush? AccentBrush
    {
        get => GetValue(AccentBrushProperty);
        set => SetValue(AccentBrushProperty, value);
    }

    public double FontSize
    {
        get => GetValue(FontSizeProperty);
        set => SetValue(FontSizeProperty, value);
    }

    public FontFamily EditorFontFamily
    {
        get => GetValue(EditorFontFamilyProperty);
        set => SetValue(EditorFontFamilyProperty, value);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);
        context.FillRectangle(Background ?? Brushes.Transparent, new Rect(Bounds.Size));

        if (Session is not { HasDocument: true } session)
        {
            DrawEmptyState(context);
            return;
        }

        var lineHeight = Math.Max(18, FontSize * 1.55);
        var firstLine = Math.Max(0, (int)Math.Floor(_verticalOffset / lineHeight));
        var visibleCount = Math.Max(1, (int)Math.Ceiling(Bounds.Height / lineHeight) + 1);
        EditorViewportSnapshot? snapshot;
        try
        {
            snapshot = session.Snapshot(firstLine, firstLine + visibleCount);
        }
        catch (Exception exception)
        {
            context.DrawText(CreateText($"Editor error: {exception.Message}", Foreground ?? Brushes.IndianRed, 13), new Point(12, 12));
            return;
        }

        if (snapshot is null)
        {
            DrawEmptyState(context);
            return;
        }
        var typeface = new Typeface(EditorFontFamily);
        var textBrush = Foreground ?? Brushes.White;
        var gutterBrush = GutterForeground ?? Brushes.Gray;
        var selectionBrush = SelectionBrush ?? new SolidColorBrush(Color.FromArgb(80, 86, 166, 184));
        var accentBrush = AccentBrush ?? Brushes.DeepSkyBlue;
        var yOrigin = -(firstLine * lineHeight - _verticalOffset);

        foreach (var line in snapshot.Lines)
        {
            var y = yOrigin + ((line.LineIndex - firstLine) * lineHeight);
            if (y + lineHeight < 0 || y > Bounds.Height)
            {
                continue;
            }

            if (snapshot.Selection.Anchor == snapshot.Selection.Head
                && IsCaretOnLine(snapshot.Selection.Head, line))
            {
                context.FillRectangle(new SolidColorBrush(Color.FromArgb(24, 128, 180, 192)), new Rect(0, y, Bounds.Width, lineHeight));
            }

            var number = new FormattedText(
                line.Number.ToString(CultureInfo.InvariantCulture),
                CultureInfo.InvariantCulture,
                FlowDirection.LeftToRight,
                typeface,
                FontSize - 1,
                gutterBrush);
            context.DrawText(number, new Point(GutterWidth - number.Width - 12, y + 1));

            DrawSelection(context, line, snapshot.Selection, y, lineHeight, typeface, selectionBrush);
            var text = new FormattedText(
                line.Text,
                CultureInfo.InvariantCulture,
                FlowDirection.LeftToRight,
                typeface,
                FontSize,
                textBrush);
            context.DrawText(text, new Point(GutterWidth + HorizontalPadding - _horizontalOffset, y));

            if (IsCaretOnLine(snapshot.Selection.Head, line))
            {
                var caretColumn = ByteColumn(line.Text, snapshot.Selection.Head - line.StartByte);
                var caretX = GutterWidth + HorizontalPadding - _horizontalOffset + MeasurePrefix(line.Text, caretColumn, typeface);
                context.FillRectangle(accentBrush, new Rect(caretX, y + 2, 1.5, Math.Max(14, lineHeight - 4)));
            }
        }
    }

    protected override void OnPropertyChanged(AvaloniaPropertyChangedEventArgs change)
    {
        base.OnPropertyChanged(change);
        if (change.Property == SessionProperty)
        {
            if (_observedSession is not null)
            {
                _observedSession.PropertyChanged -= OnSessionPropertyChanged;
            }
            _observedSession = Session;
            if (_observedSession is not null)
            {
                _observedSession.PropertyChanged += OnSessionPropertyChanged;
            }
            _verticalOffset = 0;
            _horizontalOffset = 0;
            InvalidateVisual();
        }
        else if (change.Property == FontSizeProperty || change.Property == EditorFontFamilyProperty)
        {
            InvalidateVisual();
        }
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        if (_observedSession is null && Session is not null)
        {
            _observedSession = Session;
            _observedSession.PropertyChanged += OnSessionPropertyChanged;
        }
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        if (_observedSession is not null)
        {
            _observedSession.PropertyChanged -= OnSessionPropertyChanged;
            _observedSession = null;
        }
        base.OnDetachedFromVisualTree(e);
    }

    private void OnSessionPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        InvalidateVisual();
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (e.GetCurrentPoint(this).Properties.PointerUpdateKind != PointerUpdateKind.LeftButtonPressed
            || Session is not { HasDocument: true } session)
        {
            return;
        }

        Focus();
        var position = e.GetPosition(this);
        var lineHeight = Math.Max(18, FontSize * 1.55);
        var lineIndex = Math.Max(0, (int)Math.Floor((_verticalOffset + position.Y) / lineHeight));
        var snapshot = session.Snapshot(lineIndex, lineIndex);
        var line = snapshot?.Lines.FirstOrDefault();
        if (line is null)
        {
            return;
        }

        var column = Math.Max(0, (int)Math.Round((position.X - GutterWidth - HorizontalPadding + _horizontalOffset) / Math.Max(1, FontSize * 0.62)));
        var charColumn = Math.Min(line.Text.Length, column);
        var byteOffset = line.StartByte + Encoding.UTF8.GetByteCount(line.Text.AsSpan(0, charColumn));
        session.SetSelection(byteOffset, byteOffset);
        InvalidateVisual();
        e.Handled = true;
    }

    protected override void OnPointerWheelChanged(PointerWheelEventArgs e)
    {
        base.OnPointerWheelChanged(e);
        var lineHeight = Math.Max(18, FontSize * 1.55);
        _verticalOffset = Math.Max(0, _verticalOffset - (e.Delta.Y * lineHeight * 3));
        InvalidateVisual();
        e.Handled = true;
    }

    protected override void OnTextInput(TextInputEventArgs e)
    {
        base.OnTextInput(e);
        if (Session is not { HasDocument: true } session || string.IsNullOrEmpty(e.Text))
        {
            return;
        }

        try
        {
            session.NativeSession?.InsertText(e.Text);
            InvalidateVisual();
        }
        catch (Exception exception)
        {
            System.Diagnostics.Trace.TraceWarning("Editor insert failed: {0}", exception.Message);
        }
        e.Handled = true;
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        if (Session is not { HasDocument: true } session || session.NativeSession is null)
        {
            return;
        }

        try
        {
            if (e.KeyModifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.C)
            {
                _ = CopySelectionAsync(session, cut: false);
                e.Handled = true;
                return;
            }
            if (e.KeyModifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.X)
            {
                _ = CopySelectionAsync(session, cut: true);
                e.Handled = true;
                return;
            }
            if (e.KeyModifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.V)
            {
                _ = PasteAsync(session);
                e.Handled = true;
                return;
            }
            if (e.KeyModifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.Z)
            {
                if (e.KeyModifiers.HasFlag(KeyModifiers.Shift)) session.NativeSession.Redo(); else session.NativeSession.Undo();
            }
            else if (e.KeyModifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.Y)
            {
                session.NativeSession.Redo();
            }
            else
            {
                switch (e.Key)
                {
                    case Key.Left: session.NativeSession.MoveCaret(0); break;
                    case Key.Right: session.NativeSession.MoveCaret(1); break;
                    case Key.Home: session.NativeSession.MoveCaret(2); break;
                    case Key.End: session.NativeSession.MoveCaret(3); break;
                    case Key.Back: session.NativeSession.DeleteBackward(); break;
                    case Key.Delete: session.NativeSession.DeleteForward(); break;
                    case Key.Enter: session.NativeSession.InsertText("\n"); break;
                    case Key.Tab: session.NativeSession.InsertText("    "); break;
                    default: return;
                }
            }
        }
        catch (Exception exception)
        {
            System.Diagnostics.Trace.TraceWarning("Editor command failed: {0}", exception.Message);
        }

        InvalidateVisual();
        e.Handled = true;
    }

    private async Task CopySelectionAsync(EditorViewModel session, bool cut)
    {
        var clipboard = TopLevel.GetTopLevel(this)?.Clipboard;
        if (clipboard is null || session.NativeSession is null)
        {
            return;
        }

        var text = session.NativeSession.SelectedText();
        if (text.Length == 0)
        {
            return;
        }

        await clipboard.SetTextAsync(text);
        if (cut)
        {
            session.NativeSession.DeleteBackward();
            InvalidateVisual();
        }
    }

    private async Task PasteAsync(EditorViewModel session)
    {
        var clipboard = TopLevel.GetTopLevel(this)?.Clipboard;
        if (clipboard is null || session.NativeSession is null)
        {
            return;
        }

        var text = await clipboard.TryGetTextAsync();
        if (!string.IsNullOrEmpty(text))
        {
            session.NativeSession.InsertText(text);
            InvalidateVisual();
        }
    }

    private void DrawSelection(
        DrawingContext context,
        EditorViewportLine line,
        EditorSelectionSnapshot selection,
        double y,
        double lineHeight,
        Typeface typeface,
        IBrush brush)
    {
        var start = Math.Max(line.StartByte, selection.Anchor < selection.Head ? selection.Anchor : selection.Head);
        var end = Math.Min(line.StartByte + Encoding.UTF8.GetByteCount(line.Text), selection.Anchor > selection.Head ? selection.Anchor : selection.Head);
        if (start >= end)
        {
            return;
        }

        var startColumn = ByteColumn(line.Text, start - line.StartByte);
        var endColumn = ByteColumn(line.Text, end - line.StartByte);
        var x = GutterWidth + HorizontalPadding - _horizontalOffset + MeasurePrefix(line.Text, startColumn, typeface);
        var width = Math.Max(2, MeasurePrefix(line.Text, endColumn, typeface) - MeasurePrefix(line.Text, startColumn, typeface));
        context.FillRectangle(brush, new Rect(x, y + 1, width, Math.Max(14, lineHeight - 2)));
    }

    private static bool IsCaretOnLine(int offset, EditorViewportLine line) =>
        offset >= line.StartByte && offset <= line.StartByte + Encoding.UTF8.GetByteCount(line.Text);

    private int ByteColumn(string text, int byteOffset)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        var clamped = Math.Clamp(byteOffset, 0, bytes.Length);
        return Encoding.UTF8.GetString(bytes, 0, clamped).Length;
    }

    private double MeasurePrefix(string text, int charCount, Typeface typeface)
    {
        var prefix = text[..Math.Clamp(charCount, 0, text.Length)];
        return new FormattedText(prefix, CultureInfo.InvariantCulture, FlowDirection.LeftToRight, typeface, FontSize, Foreground ?? Brushes.White).Width;
    }

    private static FormattedText CreateText(string value, IBrush brush, double size) =>
        new(value, CultureInfo.InvariantCulture, FlowDirection.LeftToRight, new Typeface(FontFamily.Default), size, brush);

    private void DrawEmptyState(DrawingContext context)
    {
        context.DrawText(
            CreateText("Open a Luau source file to begin editing.", Foreground ?? Brushes.Gray, 13),
            new Point(16, 16));
    }

}
