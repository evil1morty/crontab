# Crontab

A Windows tray-resident cron manager. Schedule any command with classic 5-field cron syntax, get per-job logs, and forget about it.

Built with Tauri 2 + WebView2 — single ~3 MB executable, no Node, no installer required.

---

## Features

- Classic 5-field cron syntax (`* * * * *`) plus `@hourly`, `@daily`, `@weekly`, `@monthly`
- Live "next run in 2h 14m" preview while you type the schedule
- Per-job log files at `%LOCALAPPDATA%\claude-cron\data\logs\<job>.log`
- **Auto-disable after N runs** — useful for one-shot or capped retries
- **Timeout per job** — kill the child after N seconds
- **Skip if previous run is still alive** (default; opt out with "Allow concurrent runs")
- **Run when Crontab launches** — fire a job once at app boot
- Master pause toggle in the tray and sidebar
- Hot-reload of the config TOML — edit it externally and changes pick up at the next minute boundary
- Persistent run history (survives restart)
- Search/filter on the jobs list
- Light + dark theme toggle, persisted
- Hide-on-close + autostart-on-login (registers `HKCU\…\Run` with `--hidden`)
- Keyboard shortcuts: `Ctrl+N` new job, `Ctrl+W` / `Esc` hide, `1` `2` `3` switch tabs

## Install

Grab the latest `Crontab.exe` (portable) or `Crontab_<version>_x64_en-US.msi` (installer) from the [Releases](https://github.com/evil1morty/crontab/releases) page.

The portable `.exe` is fully standalone — drop it anywhere and run. WebView2 ships with Windows 11; on Windows 10 it's installed by Edge.

## Build from source

```sh
cargo build --release
# binary at target\release\crontab.exe
```

For the MSI installer:

```sh
cargo install tauri-cli --locked
cargo tauri build
# msi at target\release\bundle\msi\
```

Build deps are stock Rust + the `image` crate (used by `build.rs` to generate icons on first build). No Node, no npm.

## Usage

1. Launch — the app drops into the system tray.
2. Click the tray icon (or double-click) to open the window.
3. **+ New job** — name, cron schedule, command. Save.
4. Watch runs land on the **Logs** tab.

Right-click the tray for `Show / Hide / Toggle pause all / Open logs folder / Quit`.

### Cron schedule syntax

Standard 5-field: `min hour day-of-month month day-of-week`.

| Want                         | Schedule        |
|------------------------------|-----------------|
| Every minute                 | `* * * * *`     |
| Every 15 minutes             | `*/15 * * * *`  |
| Every hour at :00            | `0 * * * *`     |
| Daily at 9:00                | `0 9 * * *`     |
| Weekdays at 9:00             | `0 9 * * 1-5`   |
| Mondays at 8:00              | `0 8 * * 1`     |
| Daily at midnight (alias)    | `@daily`        |

The form has chips for the common ones — click to fill.

### Command tips

Commands run via `cmd /C <your-command>` from the app's working directory (typically `C:\Windows\System32`). For project-relative scripts use `cd` first:

```
cd C:\path\to\project && npm run cron
```

A `.bat` script is the no-deps option:

```
"C:\Users\you\scripts\backup.bat"
```

### Files

- Config: `%APPDATA%\claude-cron\config\crontab.toml` — hand-editable, hot-reloaded
- Per-job logs: `%LOCALAPPDATA%\claude-cron\data\logs\<sanitized-name>.log`
- Run history (UI): `%LOCALAPPDATA%\claude-cron\data\history.json`

## License

MIT — see [LICENSE](LICENSE).
