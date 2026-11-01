"""
Detection of the menu bar that Win32 programs (under Wine) draw at the top of
their client area, below the title bar.

The window manager's title bar is not captured, but this menu bar is part of
the window content. It looks the same in every program: a band of one flat
background color with the menu items on its left, closed by a one or two
pixel gray separator line. :func:`detect_menu_bar` looks for exactly that
shape in the top rows of a captured BGRA frame.

Many programs also draw a sunken frame around their client area (the Win32
client edge): the separator is then the top side of a 1 or 2 pixel frame
whose left, right and bottom sides are gray lines too. When all four sides
are there, they are cropped with the bar, so only the program's image is left.

It is pure Python on purpose (no numpy dependency): it only reads the top
``_MAX_HEIGHT`` rows, sampling every few pixels.
"""

from typing import List, NamedTuple, Optional, Sequence, Tuple

Color = Tuple[int, int, int]

_MAX_HEIGHT = 64  # the bar and its separator fit in this many rows
_MIN_BAR = 10  # rows of flat background (the menu text height)
_MAX_SEPARATOR = 2  # rows of the line that closes the bar (or frame width)
_STEP = 3  # sample every _STEP pixels along a row
_TOLERANCE = 16  # max channel difference from a row's median color
_FLAT_SHARE = 0.9  # share of samples near the median for a flat row
_SEPARATOR_CONTRAST = 24  # min luma difference between bar and separator
_MIN_TEXT = 0.01  # min share of non-background samples in the items region
_ITEMS_SHARE = 0.45  # left part of the row where the menu items are
_FLAT_FROM = 0.55  # right part of the row that must be plain background
_GRAY = 12  # max channel spread of the gray separator and frame lines
_EDGE_TOLERANCE = 8  # max channel difference along a frame line


def _pixel(row: memoryview, x: int) -> Color:
    """(r, g, b) of pixel *x* in a BGRA row."""
    i = x * 4
    return row[i + 2], row[i + 1], row[i]


def _close(a: Color, b: Color, tolerance: int = _TOLERANCE) -> bool:
    return all(abs(p - q) <= tolerance for p, q in zip(a, b))


def _luma(c: Color) -> float:
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def _gray(c: Color) -> bool:
    return max(c) - min(c) <= _GRAY


class MenuBarCrop(NamedTuple):
    """Pixels to crop on each side; all zero when there is no menu bar."""

    top: int = 0
    left: int = 0
    right: int = 0
    bottom: int = 0


def _flat_color(samples: Sequence[Color]) -> Optional[Color]:
    """
    The median color of *samples* if nearly all of them are close to it,
    else None. The share leaves room for a cursor or an overlapping popup.
    """
    mid = len(samples) // 2
    median = tuple(sorted(s[i] for s in samples)[mid] for i in range(3))
    near = sum(_close(median, s) for s in samples)
    return median if near >= _FLAT_SHARE * len(samples) else None


def detect_menu_bar(frame, width: int, height: int) -> MenuBarCrop:
    """
    Find a Wine menu bar at the top of *frame* (BGRA, ``width * 4`` bytes per
    row) and return how much to crop on each side, or all zeros.

    The rows must be, from the top: at least ``_MIN_BAR`` rows whose right
    part is one flat background color and whose left part holds some menu
    text, then 1 to ``_MAX_SEPARATOR`` flat gray rows of a clearly different
    brightness (the separator). The top crop includes the separator. When
    the separator is the top side of a client-edge frame, the frame's other
    sides are cropped too.
    """
    if width < 64 or height < _MAX_HEIGHT * 2:
        return MenuBarCrop()
    view = memoryview(frame).cast("B")
    stride = width * 4
    rows = min(_MAX_HEIGHT, height // 4)

    flat_xs = list(range(int(width * _FLAT_FROM), width - 4, _STEP))
    item_xs = list(range(2, int(width * _ITEMS_SHARE), _STEP))

    def row(y: int) -> memoryview:
        return view[y * stride : (y + 1) * stride]

    def flat(y: int) -> Optional[Color]:
        r = row(y)
        return _flat_color([_pixel(r, x) for x in flat_xs])

    background = flat(0)
    if background is None:
        return MenuBarCrop()

    # The bar: rows whose right part keeps the background color
    bar = 0
    text_samples = 0
    total_samples = 0
    for y in range(rows):
        color = flat(y)
        if color is None or not _close(color, background):
            break
        r = row(y)
        items: List[Color] = [_pixel(r, x) for x in item_xs]
        text_samples += sum(not _close(c, background, 40) for c in items)
        total_samples += len(items)
        bar += 1
    if bar < _MIN_BAR or bar >= rows:
        return MenuBarCrop()
    if total_samples == 0 or text_samples / total_samples < _MIN_TEXT:
        return MenuBarCrop()  # a plain band with no menu items: not a menu bar

    # The separator: at most _MAX_SEPARATOR flat gray rows of a clearly
    # different brightness. The frame is captured unscaled, so the line is
    # sharp; more rows like that are the program's own image (a dark scene).
    separator: List[Color] = []
    for y in range(bar, min(bar + _MAX_SEPARATOR, rows)):
        color = flat(y)
        if (
            color is None
            or not _gray(color)
            or abs(_luma(color) - _luma(background)) < _SEPARATOR_CONTRAST
        ):
            break
        separator.append(color)
    if not separator:
        return MenuBarCrop()
    top = bar + len(separator)

    edge = _client_edge(view, width, height, top, separator)
    return MenuBarCrop(top, edge, edge, edge)


def _client_edge(
    view: memoryview, width: int, height: int, top: int, separator: List[Color]
) -> int:
    """
    Width of the client-edge frame whose top side is *separator* (0 if it is
    not one): its left columns repeat the separator colors, and its right
    columns and bottom rows are matching gray lines.
    """
    stride = width * 4
    e = len(separator)
    ys = list(range(top, height - e, _STEP * 5))
    xs = list(range(e, width - e, _STEP * 5))

    def px(x: int, y: int) -> Color:
        i = y * stride + x * 4
        return view[i + 2], view[i + 1], view[i]

    def line(points: List[Tuple[int, int]], color: Color) -> bool:
        return all(_close(px(x, y), color, _EDGE_TOLERANCE) for x, y in points)

    for i in range(e):
        # Left side: the same colors as the top side
        if not line([(i, y) for y in ys], separator[i]):
            return 0
        # Right side and bottom: one gray color per line, the same on both
        color = px(width - 1 - i, top)
        if not _gray(color):
            return 0
        if not line([(width - 1 - i, y) for y in ys], color):
            return 0
        if not line([(x, height - 1 - i) for x in xs], color):
            return 0
    return e
