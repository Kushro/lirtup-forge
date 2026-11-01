from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any, Dict, List, Optional, Tuple

from PySide6.QtCore import QObject, QSettings, QTimer
from PySide6.QtGui import QAction, QActionGroup, QKeySequence, QIcon, QPixmap
from PySide6.QtWidgets import QMenu, QSystemTrayIcon

from ..icons import load_icon
from ...identity import APP_NAME
from ...window import get_window_icon, list_windows, WindowInfo

if TYPE_CHECKING:
    from .daemon import DaemonController
    from ..main import MainWindow

logger = logging.getLogger(__name__)


def _get_signature(
    windows: List[WindowInfo],
    session_active: bool,
    daemon_active: bool,
    close_to_tray: bool,
    minimize_to_tray: bool,
    main_window_visible: bool,
    session_target: Optional[Tuple[int, str]] = None,
    profiles: Tuple[str, ...] = (),
    active_profile: Optional[str] = None,
) -> Tuple:
    """
    Build a hashable signature representing the current menu state.

    The window list is sorted by (handle, title) to ensure a stable
    order and to detect any change in the set of visible windows.
    """
    window_sig = tuple(sorted((w.handle, w.title or "") for w in windows))
    return (
        window_sig,
        session_active,
        daemon_active,
        close_to_tray,
        minimize_to_tray,
        main_window_visible,
        session_target,
        profiles,
        active_profile,
    )


class TrayController(QObject):
    """Manages the system tray icon and its context menu."""

    def __init__(
        self,
        main_window: MainWindow,
        daemon_ctrl: DaemonController,
        parent: Optional[QObject] = None,
    ) -> None:
        """
        Args:
            main_window: The application's main window.
            daemon_ctrl: The daemon controller (for toggling daemon mode).
            parent: Optional QObject parent.
        """
        super().__init__(parent)
        self._main_window = main_window
        self._daemon_ctrl = daemon_ctrl
        self._settings = QSettings("linux-rt-upscaler")

        # Cache used for change detection
        self._daemon_action: Optional[QAction] = None
        self._cached_signature: Optional[Tuple] = None
        self._icon_cache: Dict[int, QIcon] = {}

        # Create the tray icon
        self.tray_icon = QSystemTrayIcon(load_icon("app/app", 64, 64), self)
        self.tray_icon.setToolTip(APP_NAME)

        # Build a persistent QMenu; its contents are refreshed as needed
        self._menu = QMenu()
        self._menu.setToolTipsVisible(True)
        self._submenus: List[QMenu] = []  # deleted on each rebuild
        self._menu.aboutToShow.connect(self._update_daemon_check_state)
        self.tray_icon.setContextMenu(self._menu)

        # Tray icon activation
        self.tray_icon.activated.connect(self._on_tray_activated)

        # Timer to periodically check for changes and refresh the menu
        # while it is not open.
        self._check_timer = QTimer(self)
        self._check_timer.setInterval(2000)  # ms
        self._check_timer.timeout.connect(self.refresh_menu)

        # Initial build
        self._rebuild_menu()

    # ------------------------------------------------------------------
    #  Public API
    # ------------------------------------------------------------------
    def show(self) -> None:
        """Show the tray icon and start the change-detection timer."""
        self.tray_icon.show()
        self._check_timer.start()

    def hide(self) -> None:
        """Hide the tray icon and stop the timer."""
        self.tray_icon.hide()
        self._check_timer.stop()

    # ------------------------------------------------------------------
    #  Change detection
    # ------------------------------------------------------------------
    def _get_visible_windows(self) -> List[WindowInfo]:
        """
        Return a list of windows to display, excluding the GUI itself.

        If enumeration fails, an empty list is returned and a warning is
        logged.
        """
        gui_handle = self._main_window.winId()
        try:
            windows = list_windows()
        except Exception:
            logger.exception("Failed to list windows for tray menu")
            return []
        return [w for w in windows if w.handle != gui_handle]

    def _get_tray_option_states(self) -> Tuple[Any, Any]:
        """Return current (close_to_tray, minimize_to_tray) settings."""
        return (
            self._settings.value("tray/close_to_tray", False, type=bool),
            self._settings.value("tray/minimize_to_tray", False, type=bool),
        )

    def _is_main_window_visible(self) -> bool:
        """Check if the main window is visible."""
        return self._main_window.isVisible() and not self._main_window.isMinimized()

    def refresh_menu(self) -> None:
        """
        Rebuild the menu only if the cached signature differs from the
        current state. Does nothing while the menu is visible to avoid
        native rendering glitches.
        """
        if self._menu.isVisible():
            return

        # Gather current state
        windows = self._get_visible_windows()
        session_active = self._main_window.manual_session is not None
        daemon_active = self._daemon_ctrl.active
        close_to_tray, minimize_to_tray = self._get_tray_option_states()
        main_window_visible = self._is_main_window_visible()

        new_signature = _get_signature(
            windows,
            session_active,
            daemon_active,
            close_to_tray,
            minimize_to_tray,
            main_window_visible,
            *self._session_and_profile_state(),
        )

        if new_signature != self._cached_signature:
            self._cached_signature = new_signature
            self._rebuild_menu(
                windows=windows,
                session_active=session_active,
                daemon_active=daemon_active,
                close_to_tray=close_to_tray,
                minimize_to_tray=minimize_to_tray,
                main_window_visible=main_window_visible,
            )

    # ------------------------------------------------------------------
    #  Menu construction
    # ------------------------------------------------------------------
    def _rebuild_menu(
        self,
        windows: Optional[List[WindowInfo]] = None,
        session_active: Optional[bool] = None,
        daemon_active: Optional[bool] = None,
        close_to_tray: Optional[bool] = None,
        minimize_to_tray: Optional[bool] = None,
        main_window_visible: Optional[bool] = None,
    ) -> None:
        """
        Rebuild the entire tray menu from scratch.

        This method should only be called while the menu is not visible.
        If optional arguments are omitted, current values are fetched.
        """
        # If no explicit data is provided, gather everything
        if windows is None:
            windows = self._get_visible_windows()
        if session_active is None:
            session_active = self._main_window.manual_session is not None
        if daemon_active is None:
            daemon_active = self._daemon_ctrl.active
        if close_to_tray is None or minimize_to_tray is None:
            close_to_tray, minimize_to_tray = self._get_tray_option_states()
        if main_window_visible is None:
            main_window_visible = self._is_main_window_visible()

        # Update the cached signature for consistency
        self._cached_signature = _get_signature(
            windows,
            session_active,
            daemon_active,
            close_to_tray,
            minimize_to_tray,
            main_window_visible,
            *self._session_and_profile_state(),
        )

        # Clear and rebuild the menu (clear() keeps the submenus alive)
        self._menu.clear()
        for submenu in self._submenus:
            submenu.deleteLater()
        self._submenus = []

        # --------------------------------------------------------------
        # Dynamic window list (only when no manual session is active)
        # --------------------------------------------------------------
        if not session_active:
            self._add_window_actions(self._menu, windows, self._start_upscaling)
            if windows:
                self._menu.addSeparator()
        else:
            target = self._main_window.manual_session.window_info
            header = self._menu.addAction(
                self.tr("Upscaling: {0}", "Tray menu header; {0} is a window title")
                .format(self._window_title(target) if target else "?")
            )
            header.setEnabled(False)
            restart_action = self._menu.addAction(
                self.tr("Restart", "Tray menu: restart upscaling with current settings")
            )
            restart_action.setToolTip(
                self.tr(
                    "Upscale the same window again with the current settings.",
                    "Tray menu tooltip",
                )
            )
            restart_action.triggered.connect(self._restart_upscaling)
            others = [w for w in windows if target is None or w.handle != target.handle]
            if others:
                switch_menu = self._menu.addMenu(
                    self.tr("Switch to", "Tray submenu: upscale another window")
                )
                self._submenus.append(switch_menu)
                self._add_window_actions(switch_menu, others, self._switch_upscaling)

        # --------------------------------------------------------------
        # Show / Stop action (single action whose text/icon changes)
        # --------------------------------------------------------------
        if session_active:
            stop_action = self._menu.addAction(
                self.tr("Stop", "Stop action for tray icon menu")
            )
            stop_action.setIcon(
                load_icon(
                    "actions/stop",
                    16,
                    16,
                    color=self._main_window.gui_config.palette.icon,
                    disabled_color=self._main_window.gui_config.palette.text_subtle,
                )
            )
            stop_action.triggered.connect(self._stop_upscaling)
        else:
            if main_window_visible:
                label = self.tr("Hide", "Hide action for tray icon menu")
                icon_name = "actions/hide"
                handler = self._hide_main_window
            else:
                label = self.tr("Show", "Show action for tray icon menu")
                icon_name = "actions/show"
                handler = self._show_main_window

            action = self._menu.addAction(label)
            action.setIcon(
                load_icon(
                    icon_name,
                    16,
                    16,
                    color=self._main_window.gui_config.palette.icon,
                    disabled_color=self._main_window.gui_config.palette.text_subtle,
                )
            )
            action.triggered.connect(handler)

        # --------------------------------------------------------------
        # Profile used for the next session (restarts a running one)
        # --------------------------------------------------------------
        self._add_profile_menu()

        # --------------------------------------------------------------
        # Daemon Mode toggle
        # --------------------------------------------------------------
        daemon_action = self._menu.addAction(
            self.tr("Daemon mode", "Daemon mode toggle for tray icon menu")
        )
        daemon_action.setCheckable(True)
        daemon_action.setChecked(daemon_active)
        daemon_action.toggled.connect(self._main_window.set_daemon_mode)
        daemon_action.toggled.connect(lambda _: self.refresh_menu())
        self._daemon_action = daemon_action

        self._menu.addSeparator()

        # --------------------------------------------------------------
        # Tray options
        # --------------------------------------------------------------
        close_to_tray_action = self._menu.addAction(
            self.tr("Close to tray", "Tray menu option")
        )
        close_to_tray_action.setCheckable(True)
        close_to_tray_action.setChecked(close_to_tray)
        close_to_tray_action.toggled.connect(
            lambda checked: self._settings.setValue("tray/close_to_tray", checked)
        )

        minimize_to_tray_action = self._menu.addAction(
            self.tr("Minimize to tray", "Tray menu option")
        )
        minimize_to_tray_action.setCheckable(True)
        minimize_to_tray_action.setChecked(minimize_to_tray)
        minimize_to_tray_action.toggled.connect(
            lambda checked: self._settings.setValue("tray/minimize_to_tray", checked)
        )

        start_hidden_action = self._menu.addAction(
            self.tr("Start hidden", "Tray menu option")
        )
        start_hidden_action.setCheckable(True)
        start_hidden_action.setChecked(
            bool(self._settings.value("tray/start_hidden", False, type=bool))
        )
        start_hidden_action.toggled.connect(
            lambda checked: self._settings.setValue("tray/start_hidden", checked)
        )

        keep_running_action = self._menu.addAction(
            self.tr("Keep running after exit", "Tray menu option")
        )
        keep_running_action.setCheckable(True)
        keep_running_action.setChecked(
            bool(self._settings.value("tray/keep_running_on_exit", False, type=bool))
        )
        keep_running_action.toggled.connect(
            lambda checked: self._settings.setValue(
                "tray/keep_running_on_exit", checked
            )
        )

        self._menu.addSeparator()

        # --------------------------------------------------------------
        # Exit
        # --------------------------------------------------------------
        exit_action = self._menu.addAction(self.tr("Exit"))
        exit_action.setIcon(
            load_icon(
                "actions/exit",
                16,
                16,
                color=self._main_window.gui_config.palette.icon,
                disabled_color=self._main_window.gui_config.palette.text_subtle,
            )
        )
        exit_action.setShortcut(QKeySequence("Ctrl+Q"))
        exit_action.triggered.connect(self._quit_app)

    # ------------------------------------------------------------------
    #  Slots / helpers
    # ------------------------------------------------------------------
    def _on_tray_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        """Handle double-click (or platform equivalent) to show the window."""
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            if self._is_main_window_visible():
                self._hide_main_window()
            else:
                self._show_main_window()

    def _update_daemon_check_state(self) -> None:
        """Update the state of the daemon checkbox."""
        if self._daemon_action is not None:
            self._daemon_action.blockSignals(True)
            self._daemon_action.setChecked(self._daemon_ctrl.active)
            self._daemon_action.blockSignals(False)

    def _get_window_icon(self, handle: int) -> QIcon:
        """Return a cached QIcon for the given window, fetching it if needed."""
        if handle not in self._icon_cache:
            img = get_window_icon(handle, size=16)
            if img is not None and not img.isNull():
                self._icon_cache[handle] = QIcon(QPixmap.fromImage(img))
            else:
                self._icon_cache[handle] = QIcon()  # empty icon
        return self._icon_cache[handle]

    def _session_and_profile_state(
        self,
    ) -> Tuple[Optional[Tuple[int, str]], Tuple[str, ...], Optional[str]]:
        """Session target, profile names and active profile (menu signature)."""
        session = self._main_window.manual_session
        target = None
        if session is not None and session.window_info is not None:
            target = (session.window_info.handle, session.window_info.title or "")
        return (
            target,
            tuple(self._main_window.profile_names),
            self._main_window.active_profile_name,
        )

    def _window_title(self, win: WindowInfo) -> str:
        return win.title or self.tr("Unknown", "Fallback if no window title was found")

    def _add_window_actions(self, menu: QMenu, windows: List[WindowInfo], slot) -> None:
        """Add one action per window to *menu*, calling *slot(window)*."""
        for win in windows:
            action = menu.addAction(self._window_title(win))
            action.setIcon(self._get_window_icon(win.handle))
            action.setData(win)
            action.triggered.connect(lambda checked=False, w=win: slot(w))

    def _add_profile_menu(self) -> None:
        """Radio list of profiles; Global auto-matches a profile per window."""
        menu = self._menu.addMenu(self.tr("Profile", "Tray submenu"))
        self._submenus.append(menu)
        group = QActionGroup(menu)
        active = self._main_window.active_profile_name
        names: List[Optional[str]] = [None] + self._main_window.profile_names
        for name in names:
            label = name or self.tr(
                "Global (auto-match)", "Tray menu: global settings, no profile"
            )
            action = menu.addAction(label)
            action.setCheckable(True)
            action.setChecked(name == active)
            group.addAction(action)
            action.triggered.connect(
                lambda checked=False, n=name: self._select_profile(n)
            )

    def _select_profile(self, name: Optional[str]) -> None:
        self._main_window.select_profile(name)
        self.refresh_menu()

    def _restart_upscaling(self) -> None:
        """Upscale the same window again with the current settings."""
        self._main_window.restart_manual_session()
        self.refresh_menu()

    def _switch_upscaling(self, win_info: WindowInfo) -> None:
        """Stop the running session and upscale *win_info* instead."""
        self._main_window.restart_manual_session(win_info)
        self.refresh_menu()

    def _start_upscaling(self, win_info: WindowInfo) -> None:
        """Start a manual upscaling session for the given window."""
        self._main_window._on_window_selected(win_info)
        # Immediate refresh because session state changed
        self.refresh_menu()

    def _stop_upscaling(self) -> None:
        """Stop the active manual session."""
        self._main_window.stop_manual_session()
        self.refresh_menu()

    def _hide_main_window(self) -> None:
        """Hide the main window."""
        self._main_window.hide_gui()
        self.refresh_menu()

    def show_upscaling_message(self):
        """Show a message, informing upscaling is in progress and which window."""
        session = self._main_window.manual_session
        if session is not None and session.window_info is not None:
            title = session.window_info.title
            message = self.tr(
                "Upscaling: '{0}'.\nUse Stop from the tray menu to return.",
                "Tray message if a window was already being upscaled. "
                "{0} is the window title placeholder.",
            ).format(title)
        else:
            message = self.tr(
                "Upscaling in progress.\nUse Stop from the tray menu to return.",
                "Tray message if a window was already being upscaled "
                "and no window title was available to show.",
            )

        self.tray_icon.showMessage(
            APP_NAME,
            message,
            QSystemTrayIcon.MessageIcon.Information,
            5000,
        )

    def _show_main_window(self) -> None:
        """Show the main window, unless a manual session is active."""
        if self._main_window.manual_session is not None:
            self.show_upscaling_message()
            return
        self._main_window.show_gui()
        self.refresh_menu()

    def _quit_app(self) -> None:
        """Clean up and quit the application."""
        self._main_window.request_quit()
