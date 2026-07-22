# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), this project adheres to
[Semantic Versioning](https://semver.org/), and entries are grouped by minor version.

## [0.5] - 2026-07-22

### Added

- **Widget grid docking**: a new "Placement" tab in the Widget style window lets you snap several
  widgets into a grid; dragging any docked widget moves the whole group together. Column count and
  placement order are configurable.
- **Antigravity IDE quota widget**: a third monitored service tracking Antigravity IDE's (local
  language_server API) coding quota. No sign-in required: it automatically detects the running
  IDE. Shows 5-hour/weekly usage for both the Gemini model group and the Claude/GPT model group,
  with a toggle for which one the widget headlines. Unofficial API, Windows-only.

## [0.4] - 2026-07-22

### Added

- **Multi-service monitoring**: alongside Claude, SessionMeter can now track **Antigravity
  (Gemini subscription)** usage. Sign-in happens in a dedicated window running in a separate
  process, so a problem loading the sign-in page can't affect the rest of the app. There is no
  official API, so this relies on screen scraping and is offered as experimental.
- **Widget style window**: pick, **per service**, from 10 styles: five graphic concepts
  (horizontal bars, concentric rings, dual semi-arc, vertical pillars, hex rings), each in a
  detailed or compact variant. Includes a live preview and a remaining/used display toggle.
- **Per-service widgets**: each signed-in service gets its own widget window, with the service
  name shown in the title bar.
- **Per-service statistics tabs**: the statistics window separates each signed-in service into
  its own tab.
- **Separate dev build**: running a development build uses a distinct identifier (`-dev`) so it
  runs alongside the installed release without conflicting.
- **Service icon in widget titles**: each widget title now shows a brand-coloured service icon so
  the service is identifiable at a glance.

### Changed

- Widget settings moved from the settings window into the new **Widget style** window; the old
  detailed/compact toggle is replaced by the 10-style picker.
- The settings account section is now a **per-service sign-in/out list**.
- The tray menu's widget show/hide row is removed; each service's widget is now shown or hidden
  individually from the **Widget style** window.
- The compact widget styles show the time remaining in a clock format (`H:MM` / `M:SS`).
- The app checks for updates on startup and **every 10 minutes** (previously startup only).
- Renamed the **Antigravity** service to **Gemini** (display name and internal id); existing
  session, settings, widget position, and history are migrated automatically.
- The settings account list now shows each service's **account and subscription plan** together;
  Gemini's Google account email is captured best-effort on sign-in.

### Fixed

- **Widget position no longer resets after an update**: each widget's position is saved right
  before the auto-update restart, so widgets return to where you left them.
- **The tray menu's Quit item was sometimes clipped off-screen**: the menu window now sizes to
  its content.
- **Stabilized update delivery**: releases are published as a tagged prerelease instead of a
  draft, preventing the update installer URL from detaching from the version tag and 404-ing.
- **Fixed the statistics window sometimes not showing other services' tabs**: the signed-in
  service list is re-read when the window gains focus, so a tab missed due to startup timing
  (e.g. Gemini) now appears.
- **Fixed settings (widget style, refresh interval) resetting after an update**: a pre-0.4
  settings file is now rewritten to the current format on first launch instead of being
  re-derived every launch, and the settings/widget-style windows re-read the latest settings on
  focus so a change in one window can't be reverted by a stale save in another.
- **Fixed the app reverting to an older version after an update followed by a reboot**: when the
  autostart (launch-at-startup) entry pointed at a path that no longer matched the real
  executable, a reboot relaunched the stale old binary. On startup, if autostart is enabled, the
  app now re-registers it against the current executable path, self-healing the stale entry.
- **Fixed the widget style resetting to default after an update**: saving settings replaced the
  whole per-service widget map; it now merges, keeping the stored widget config for any service
  the save omits, so a settings save that runs before the widget config has loaded can no longer
  wipe the user's widget styles.
- **Fixed the what's-new window showing only up to version 0.3**: the fallback used when the
  GitHub fetch fails was a hand-maintained copy that had gone stale; it now bundles the actual
  changelog at build time, so it always matches the installed version. Multi-line entries are no
  longer truncated at their first line.

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

[0.4]: https://github.com/soma0sd/Session-Meter/releases
[0.3]: https://github.com/soma0sd/Session-Meter/releases
[0.2]: https://github.com/soma0sd/Session-Meter/releases
[0.1]: https://github.com/soma0sd/Session-Meter/releases
