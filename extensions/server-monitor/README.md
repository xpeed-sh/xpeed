# Server monitor 0.1.2

**Version:** `0.1.2` · [Changelog](CHANGELOG.md) · [VERSION](VERSION)

Optional MIT-licensed extension for Xpeed. Shows CPU, RAM, root disk usage, traffic for the first four network interfaces other than `lo`, and processes consuming the most CPU or RAM, with up to six minutes of charts.

## Use

Install **Server monitor** from Extensions. Click **Open Monitor**, then **Connect extension**. The panel contains **Server**, with connection information, and **Monitor**, with metrics and charts. It has one close action for the whole panel and an expand/restore action; it adds no footer icon.

Select an authenticated SSH tab: the monitor uses its host, port, username and login. Switching sessions clears the old charts and changes server. Local terminals and ended sessions have no remote metrics. Sampling pauses while Monitor is hidden or Server is selected, and resumes when you return.

The extension follows **Settings → Language**. English is the default; Spanish is supported, including changes while connected. Process names, server names and network interface names are preserved. See [README.es.md](README.es.md) for Spanish documentation.

Samples arrive every three seconds through a persistent SSH channel independent of the terminal. Credentials remain in Xpeed; the extension receives only samples and server identification. The channel uses the private key, SSH agent or password already entered for the session. Additional interactive authentication such as MFA may prevent the metrics channel from opening; the error is displayed.

Requires Linux with `/proc`, `sh`, `cat`, `df` and `getconf`. No agent installation or `sudo` is needed. Visible processes depend on SSH user permissions and server `/proc` restrictions. It does not modify files, kill processes or collect command arguments. Select a row to view details and copy its PID; CPU/RAM buttons change sorting.

## Metrics

- Server and process CPU: percentages between samples, normalized to total server capacity. The first sample waits for the next to calculate rates.
- RAM: total minus available memory; process RSS uses the server page size.
- Disk: used space in `/`; does not measure latency or IOPS.
- Network: received/transmitted bytes per second per interface, without adding physical and virtual interfaces together.
- Processes: union of the top 50 by CPU and top 50 by RAM, with 20 rows displayed. Search filters that set. Reused PIDs are distinguished by start time.
- Errors remain errors rather than zero-valued samples. Failed channels retry after ten seconds.

Counter interpretation follows the [Linux procfs documentation](https://www.kernel.org/doc/html/latest/filesystems/proc.html) and [`/proc/PID/stat` reference](https://www.man7.org/linux/man-pages/man5/proc_pid_stat.5.html).

## Build

From Xpeed's source, run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-git-repositories.ps1`. Publish `dist/git-repositories/xpeed.server-monitor` as a public Git repository, including `bin/`. In Xpeed 0.7.1 or later, use **Extensions → Git → Connect repository** to view its details and install directly.

Inside the standalone project run `cargo build --release --locked`. On Windows copy `target/release/xpeed-server-monitor-extension.exe` to `bin/xpeed-server-monitor-extension.exe` before publishing. For Linux/macOS use the manifest's `entrypoints` path and execution permission. The localization library and catalogs are included under `vendor/xpeed-i18n`. Xpeed does not compile projects during installation.

## Protocol

Declares `serverMetrics` and `localization`, permission `server.metrics.read`, and the right-hand `monitor` view. Xpeed sends `server/context` and `server/sample`; the adapter returns `views/updated` with a structured `dashboard`. `initialize.language` and `locale/change` select the interface language. Context identifiers change on session switching or pause/resume; outdated samples are rejected. The host accepts only bounded, finite chart data. The adapter never opens its own SSH connections.

## Releases

Keep VERSION, Cargo.toml and extension.json in sync. Update Cargo.lock with Cargo, record adapter changes in CHANGELOG.md and tag releases as `vX.Y.Z`. Host UI changes use Xpeed's version. Standalone exports include these files; rebuild and copy the platform binary to the manifest entrypoint before publishing.
