# Local plugin packages (experimental)

The first plugin-manager slice is a shared Rust API (`winarchy::plugins`) and
`winarchyctl plugin`. There is no GUI, catalogue download, GitHub installation,
installer hook, automatic migration or update checker yet.

**Upgrade the daemon together with the CLI.** Older daemons ignore the new
`plugins.toml` disabled list and the transaction gate. Do not use these mutation
commands alongside an older running daemon. Start with an isolated configuration;
the implementation does not deploy or alter your real installation automatically.

## Inventory

```powershell
winarchyctl plugin list
winarchyctl plugin list --json
winarchyctl plugin inspect applet devlauncher
winarchyctl plugin inspect theme dracula
```

Discovery is local, read-only and works offline, including plugins outside any
catalogue. `_template` is hidden. Origins are `bundled`, `local-package` and
`unmanaged`; a matching name alone never establishes provenance or a version.
Existing installations have an unknown version and remain unmanaged. Bundled
identifiers identify the bundled slot, not a guarantee that its files are pristine.
Malformed manifests are reported as errors rather than silently omitted.

Statuses describe configured activation, not provider health: `enabled` does not
prove that an external service is running or a popup compiles. The bar itself may
also be globally disabled. Themes distinguish `active` from merely `installed`.

## Local package format

A package is explicitly prepared; pointing at an arbitrary applet repository is
not an installation protocol:

```text
todo-package/
  plugin.toml
  payload/
    applet.toml
    view.slint
    todo.ps1
    icon.svg
    ...other runtime assets...
```

```toml
schema = 1
id = "todo"
kind = "applet" # or "theme"
version = "0.1.0"
name = "Todo"
description = "Local task list"
```

The package directory name is irrelevant. IDs use 1–32 lowercase ASCII letters,
digits, hyphens or underscores; Windows device names, built-in plugin IDs and bar
module names are reserved. Metadata is separate from the strict runtime manifest.
Version is a bounded label: identical versions are refused on update, but there is
**no semantic ordering or downgrade protection** in this first local API.

For themes, `payload/` contains `theme.toml`, optional `preview.png`/JPG,
`wallpapers/`, `README.md`, `LICENSE` and `SOURCES.md`, using the existing
[theme pack](themes.md#external-theme-packs) format and validator. The package ID
becomes the installed theme ID. Unrecognized theme payload files are not installed.

For applets, include only runtime files, not build trees, caches, personal data,
installers or tests. All payload files are copied, including nested view assets.
The runtime manifest, interval, provider, popup bounds and script presence are
validated. Slint compilation and external command/dependency availability are
**not** validated by the offline installer. Full `command` providers are preserved,
not rewritten or installed elsewhere. Native/WSL packaging adapters are future
work; absolute user-specific paths are not made portable automatically.

Packages must be trusted: providers are executable code, with the user's rights
when enabled. The manager never executes their providers or installer scripts.
There is no sandbox or signature verification.

## Operations

These commands use `%USERPROFILE%\.config\winarchy`, or `WINARCHY_CONFIG_HOME`.
The selected directory must already be a valid initialized Winarchy configuration.
Local management also runs on Linux/WSL with an explicit config home; Windows
commands such as `theme set` still require the Windows CLI and running daemon.

```powershell
# Installs files, but does not activate the applet or alter the bar.
winarchyctl plugin install C:\Packages\todo-package

# Existing placement is retained; a new standalone applet defaults to right.
winarchyctl plugin enable applet todo --section drawer
winarchyctl plugin disable applet todo
winarchyctl plugin enable applet todo

# For this first implementation, disable before updating/removing.
winarchyctl plugin disable applet todo
winarchyctl plugin update C:\Packages\todo-package-v2
winarchyctl plugin enable applet todo

winarchyctl plugin disable applet todo
winarchyctl plugin uninstall applet todo
```

`--section` is used only when adding a previously unreferenced standalone applet.
It does not move existing references. Choosing `drawer` requires an existing
configured drawer. Disable preserves all section positions, comments, settings,
files and data; enable restores the same references. On uninstall, all references
to the applet are removed; removing the last drawer item removes its empty anchor.

Attached applets are disabled through `plugins.toml`, not by deleting their
`attach` key or hiding the built-in module. Enabling one requires its host module
and refuses an already enabled attachment on that target. `time` may be hosted by
`clock`. Disabling an agenda does not arbitrarily re-enable another calendar.
Existing unmanaged and bundled applets may also be enabled/disabled; this does
not adopt them into package management.

Disabling an applet stops its Winarchy scheduling on reload, not services it has
started: dev servers, a dictation model or a Claude statusline relay remain outside
this lifecycle. An already running provider/action may finish. Disable persists a configuration
change; it is not an acknowledgment that the daemon has finished reloading.
Until the desktop lifecycle has been smoke-tested, stop the daemon for package
updates/removals, especially those involving native executables. Built-in modules
such as CPU or the clock itself are bar configuration, not removable plugins.

For themes:

```powershell
winarchyctl plugin install C:\Packages\sample-theme
winarchyctl theme set sample-theme
# Switch away before updating/uninstalling the active theme.
winarchyctl theme set catppuccin-mocha
winarchyctl plugin update C:\Packages\sample-theme-v2
winarchyctl plugin uninstall theme sample-theme
```

Applying a theme remains the existing `theme set` operation. Built-in packages
cannot be installed over, updated or uninstalled. Missing built-in files would
otherwise be recreated by Winarchy's defaults installer.

## Preservation and receipts

Managed packages have receipts under `.plugins/records/`, recording the package
metadata, local source path, SHA-256 installed file fingerprints and the upstream
applet manifest baseline. No receipt is inferred for an old installation; update
and uninstall refuse unmanaged packages. Explicit adoption is not implemented yet.

Updates:

- Refuse modified, missing or additional files, except `applet.toml` overrides.
- Merge applet manifest values three ways: previous upstream, installed local,
  new upstream. Unrelated local settings/attachments survive repeated updates;
  conflicting edits stop the operation. TOML formatting/comments in this manifest
  may be normalized; semantic values are preserved.
- Never blindly replace a locally edited palette, script, view or wallpaper tree.
- Keep bar placement and disabled state. Update does not re-enable the applet.

Uninstall refuses any difference from its installed fingerprints, including local
manifest edits made after the last installation/update. Export/resolve such edits
before removal; no `--force` or data purge is provided. User data belongs outside
package payloads: Todo tasks and pet state, for example, are never deleted by
these operations. Personal files added inside a package directory cause refusal,
not recursive deletion of untracked content. Wallpaper choices outside the theme
folder are retained as well.

## Transactions and recovery

Operations serialize using an OS file lock. A same-volume staging directory is
validated before publication. Existing files are backed up under
`.plugins/backups/transaction-*/`; successful commands print the backup path.
Ordinary publication/validation failures restore previous files in reverse order.
Concurrent local changes detected before publication abort; changes detected
while rolling back are left untouched and require manual recovery.

During publication, `.plugins/pending.json` gates daemon configuration reloads,
so the last valid configuration stays active. Startup, new manager mutations and
normal configuration snapshots refuse an interrupted transaction rather than
accepting its partial files. A successful operation or rollback clears the gate.
This is **not a database transaction or a power-loss durability guarantee**.
Do not concurrently edit package/config files or operate multiple Winarchy
versions during management.

If interrupted or rollback fails:

1. Stop Winarchy and any manager process. Preserve the entire configuration and
   `.plugins` directory before attempting recovery.
2. Read `.plugins/pending.json`: `backup` points to the transaction directory;
   `paths[i]` is relative to config home and `existed[i]` describes its prior state.
3. To roll back, restore each prior path from `before-i` in that directory.
   For paths that did not exist, remove only artifacts confirmed to belong to this
   transaction. Inspect any concurrent user edits first; do not overwrite them.
4. Once the prior configuration/receipts are restored, archive the journal and
   remove `.plugins/pending.json`, then validate/restart Winarchy.

`removed-i` holds the actual renamed originals when that publication step began;
`before-i` is the pre-publication backup. Completed and rolled-back journals remain
for inspection. Backups may contain private configuration and are not pruned
automatically. Remove old completed backups manually when satisfied; never remove
an unresolved backup. Crash-left `.plugins/stage-*` folders may be removed only
while no manager operation is running and after recovery.

Links, Windows reparse points, special files, case-colliding payload filenames and
Windows device names are refused. Payload traversal is limited to 512 entries,
16 levels and 512 MiB. Existing theme image limits also apply. Atomic transaction
gate publication requires hard links on the config volume (NTFS on Windows).
These checks are not a sandbox against a malicious concurrent filesystem writer.

## Tests

```sh
cargo test -p winarchy --lib plugins::
cargo test -p winarchy --lib disabled_direct_and_attached
cargo test -p winarchyctl
```

Tests initialize temporary configuration homes and never execute providers. They
cover the Todo-like and theme lifecycles, repeated settings preservation, drawer
placement, attached applets, local modifications, unknown installations, bad
packages, locks, interruption gates and rollback.

An optional test uses real local Todo/theme runtime sources, copying only into
its temporary homes (no modification of the sources or installed configuration):

```sh
WINARCHY_TODO_SOURCE=/path/to/winarchy-applet-collection/todo \
WINARCHY_THEME_SOURCE=/path/to/winarchy-themes/dracula \
  cargo test -p winarchy --lib local_collection_packages -- --ignored
```

Actual popup rendering, running-provider cancellation and desktop hot reload
still require a matching Windows daemon/CLI smoke test. No deployment is performed
by these tests.
