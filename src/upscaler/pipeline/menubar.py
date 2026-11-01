"""
Detection of the menu bar that Win32 programs (under Wine) draw at the top of
their client area, below the title bar.

The window manager's title bar is not captured, but this menu bar is part of
the window content. It looks the same in every program: a band of one flat
background color with the menu items on its left, closed by a one or two
pixel separator line. :func:`detect_menu_bar` looks for exactly that shape
in the top rows of a captured BGRA frame and returns how many rows to crop.

It is pure Python on purpose (no numpy dependency): it only reads the top
``_MAX_HEIGHT`` rows, sampling every few pixels.
"""

from typing import List, Optional, Sequence, Tuple

Color = Tuple[int, int, int]

_MAX_HEIGHT = 64  # the bar and its separator fit in this many rows
_MIN_BAR = 10  # rows of flat background (the menu text height)
_MAX_SEPARATOR = 4  # rows of the line that closes the bar (with blur)
_STEP = 3  # sample every _STEP pixels along a row
_TOLERANCE = 16  # max channel difference from a row's median color
_FLAT_SHARE = 0.9  # share of samples near the median for a flat row
_SEPARATOR_CONTRAST = 24  # min luma difference between bar and separator
_MIN_TEXT = 0.01  # min share of non-background samples in the items region
_ITEMS_SHARE = 0.45  # left part of the row where the menu items are
_FLAT_FROM = 0.55  # right part of the row that must be plain background


def _pixel(row: memoryview, x: int) -> Color:
    """(r, g, b) of pixel *x* in a BGRA row."""
    i = x * 4
    return row[i + 2], row[i + 1], row[i]


def _close(a: Color, b: Color, tolerance: int = _TOLERANCE) -> bool:
    return all(abs(p - q) <= tolerance for p, q in zip(a, b))


def _luma(c: Color) -> float:
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def _flat_color(samples: Sequence[Color]) -> Optional[Color]:
    """
    The median color of *samples* if nearly all of them are close to it,
    else None. The share leaves room for a cursor or an overlapping popup.
    """
    mid = len(samples) // 2
    median = tuple(sorted(s[i] for s in samples)[mid] for i in range(3))
    near = sum(_close(median, s) for s in samples)
    return median if near >= _FLAT_SHARE * len(samples) else None


def detect_menu_bar(frame, width: int, height: int) -> int:
    """
    Return the number of top rows taken by a Wine menu bar in *frame*
    (BGRA, ``width * 4`` bytes per row), or 0 when there is none.

    The rows must be, from the top: at least ``_MIN_BAR`` rows whose right
    part is one flat background color and whose left part holds some menu
    text, then 1 to ``_MAX_SEPARATOR`` flat rows of a clearly different color
    (the separator). The returned count includes the separator.
    """
    if width < 64 or height < _MAX_HEIGHT * 2:
        return 0
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
        return 0

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
        return 0
    if total_samples == 0 or text_samples / total_samples < _MIN_TEXT:
        return 0  # a plain band with no menu items: not a menu bar

    # The separator: flat rows of a clearly different color. A scaled or
    # filtered frame may blur it, adding a flat row of an in-between color
    # before it; anything that is not flat is already the content.
    end = 0
    for y in range(bar, min(bar + _MAX_SEPARATOR, rows)):
        color = flat(y)
        if color is None:
            break
        if abs(_luma(color) - _luma(background)) >= _SEPARATOR_CONTRAST:
            end = y + 1
        elif end:
            break  # back to a bar-like color after the line
    return end
