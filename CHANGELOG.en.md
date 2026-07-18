# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.2.1] - 2026-07-18

### Fixed

- Fixed the widget disappearing when a tray double-click opened the statistics window (now
  honors the system double-click time).
- Show the widget immediately on first run (even before sign-in).

### Changed

- Fetch the what's-new changelog from the GitHub repository (falls back to a bundled copy
  when offline or before the repo is published).

## [0.2.0] - 2026-07-18

### Added

- **Auto-update UI**: checks for a new version on startup and offers a one-click update from
  the widget icon button and the tray context menu (download, install, relaunch).
- **What's-new window**: shows the changelog in Korean or English per the language setting.
- Show the account name and email in the statistics window.

### Fixed

- Fixed the widget flickering (toggling) when the statistics window was opened.

## [0.1.0] - 2026-07-18

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
- **GitHub-based auto-update** (Tauri v2 updater, signature verification): automatic check on
  startup plus manual check/install in settings.

[0.2.1]: https://github.com/soma0sd/Session-Meter/releases/tag/v0.2.1
[0.2.0]: https://github.com/soma0sd/Session-Meter/releases/tag/v0.2.0
[0.1.0]: https://github.com/soma0sd/Session-Meter/releases/tag/v0.1.0
