"""Menu bar detection on synthetic BGRA frames."""

from upscaler.pipeline.menubar import MenuBarCrop, detect_menu_bar

WIDTH, HEIGHT = 400, 300
BAR = (240, 240, 240)
SEPARATOR = (160, 160, 160)
TEXT = (20, 20, 20)


def _frame(rows):
    """Build a BGRA frame from a function (x, y) -> (r, g, b)."""
    data = bytearray(WIDTH * HEIGHT * 4)
    for y in range(HEIGHT):
        for x in range(WIDTH):
            r, g, b = rows(x, y)
            i = (y * WIDTH + x) * 4
            data[i : i + 4] = bytes((b, g, r, 255))
    return bytes(data)


def _content(x, y):
    """Busy, non-flat game content."""
    v = (x * 37 + y * 91) % 256
    return (v, (v * 3) % 256, (v * 7) % 256)


def _menu_text(x, y, top=4, bottom=14):
    """Menu items: dark glyph strokes in the left part of the bar."""
    return top <= y < bottom and x < 150 and x % 9 in (1, 2)


def _with_bar(bar_rows=18, separator_rows=1, text=True, blur=False):
    def pixel(x, y):
        if y < bar_rows:
            return TEXT if text and _menu_text(x, y) else BAR
        y -= bar_rows
        if blur:
            if y == 0:
                return (200, 200, 200)
            y -= 1
        if y < separator_rows:
            return SEPARATOR
        return _content(x, y)

    return pixel


def test_detects_bar_and_separator():
    assert detect_menu_bar(_frame(_with_bar()), WIDTH, HEIGHT) == MenuBarCrop(19)


def test_two_row_separator_is_included():
    frame = _frame(_with_bar(separator_rows=2))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop(20)


def test_blurred_separator_is_included():
    frame = _frame(_with_bar(blur=True))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop(20)


def test_no_bar_in_plain_content():
    assert detect_menu_bar(_frame(_content), WIDTH, HEIGHT) == MenuBarCrop()


def test_plain_band_without_items_is_not_a_menu():
    frame = _frame(_with_bar(text=False))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop()


def test_band_too_short_is_not_a_menu():
    frame = _frame(_with_bar(bar_rows=6))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop()


def test_tiny_frames_are_ignored():
    assert detect_menu_bar(bytes(32 * 32 * 4), 32, 32) == MenuBarCrop()


# A Wine window like G-senjou no Maou: white menu bar (19 rows), then a 2 px
# sunken client-edge frame around the program's image
EDGE_DARK = ((166, 166, 166), (106, 106, 106))  # top and left sides
EDGE_LIGHT = ((255, 255, 255), (227, 227, 227))  # bottom and right, outside in
NIGHT = (29, 19, 47)  # a flat dark scene at the top of the image


def _wine_window(content):
    """The frame side nearest to a pixel wins; on a tie, the light side."""
    white = (255, 255, 255)

    def pixel(x, y):
        if y < 19:
            return TEXT if _menu_text(x, y) else white
        dark = min(y - 19, x)  # distance to the top and left sides
        light = min(HEIGHT - 1 - y, WIDTH - 1 - x)  # to the bottom and right
        if light < 2 and light <= dark:
            return EDGE_LIGHT[light]
        if dark < 2:
            return EDGE_DARK[dark]
        return content(x - 2, y - 21)

    return pixel


def test_client_edge_frame_is_cropped_on_every_side():
    frame = _frame(_wine_window(_content))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop(21, 2, 2, 2)


def test_flat_dark_scene_is_not_taken_for_the_separator():
    """The first rows of the image were counted as separator (23 instead of 21)."""
    frame = _frame(_wine_window(lambda x, y: NIGHT if y < 4 else _content(x, y)))
    assert detect_menu_bar(frame, WIDTH, HEIGHT) == MenuBarCrop(21, 2, 2, 2)


def test_separator_is_at_most_two_rows():
    black = (0, 0, 0)  # gray and contrasting, but already the image

    def pixel(x, y):
        if y < 18:
            return TEXT if _menu_text(x, y) else BAR
        if y < 20:
            return SEPARATOR
        return black if y < 24 else _content(x, y)

    assert detect_menu_bar(_frame(pixel), WIDTH, HEIGHT) == MenuBarCrop(20)


def test_colored_line_is_not_a_separator():
    def pixel(x, y):
        if y < 18:
            return TEXT if _menu_text(x, y) else BAR
        return (200, 40, 40) if y == 18 else _content(x, y)

    assert detect_menu_bar(_frame(pixel), WIDTH, HEIGHT) == MenuBarCrop()
