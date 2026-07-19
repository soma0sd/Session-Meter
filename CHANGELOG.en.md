# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), this project adheres to
[Semantic Versioning](https://semver.org/), and entries are grouped by minor version.

## [0.3] - 2026-07-19

### Added

- **Compact widget layout**: current-session and weekly usage shown as two donut charts on one
  row. Switch between detailed and compact in settings (detailed by default).
- **Subscription plan** shown in the statistics window (e.g. Claude Max 20x).
- **Time axis** across the top of the history chart.
- **Per-window alert thresholds**: set the used-% alert threshold for the current session and
  the weekly window independently. Disabling notifications also disables the sub-options.
- **Widget visibility watchdog**: re-checks the widget each refresh cycle and recovers a widget
  that drifted off-screen.

### Fixed

- **Settings and session no longer reset on update**: settings are written atomically and the
  widget position lives in a separate file, so an exit during an auto-update can no longer
  corrupt or reset them. A corrupted settings file is backed up and recovered to defaults.
- **Widget no longer stays off-screen (invisible)**: the minimized sentinel position is never
  saved, and an off-screen saved position falls back to the default corner.
- **Start Menu shortcut now shows the app icon after install**: the installer refreshes the
  shell icon cache so the custom icon appears right away (including any stale cache left by
  the earlier product name).
- History chart now plots by real time and adds a **30-day view** (24h / 7d / 30d render at
  their true scale). Fixed the "NaN" remaining time in the reset schedule.

### Changed

- Settings sections are grouped into cards. Removed the unused "tray window" option.

## [0.2] - 2026-07-18

### Added

- **Auto-update**: checks for a new version on startup and offers a one-click update from the
  widget icon button and the tray context menu (download, install, relaunch).
- **What's-new window**: shows the changelog in Korean or English per the language setting,
  fetched from the GitHub repository (falls back to a bundled copy when offline or before the
  repo is published).
- Show the account name and email in the statistics window.

### Fixed

- Fixed the widget flickering or disappearing when a tray double-click opened the statistics
  window (now honors the system double-click time).
- Show the widget immediately on first run (even before sign-in).

## [0.1] - 2026-07-18

### Added

- **Tray icon**: remaining 5-hour session usage shown by color (green/yellow/red), theme-aware.
  Left-click toggles the widget, double-click opens stats, right-click shows the custom menu.
- **Desktop widget**: 5-hour and weekly usage with reset countdown, always-on-top and
  move-lock toggles, opacity, and stats/settings shortcuts.
- **Statistics window**: all usage buckets, history chart, depletion forecast, reset schedule.
- **Custom themed context menu** and desktop notifications (80% / 95% used, reset).
- **Dark/light/system themes**, Korean/English UI (OS-locale auto-detect), launch at startup,
  single instance.
- **claude.ai session-cookie usage source**. The session is stored encrypted with Windows DPAPI.

[0.3]: https://github.com/soma0sd/Session-Meter/releases
[0.2]: https://github.com/soma0sd/Session-Meter/releases
[0.1]: https://github.com/soma0sd/Session-Meter/releases
