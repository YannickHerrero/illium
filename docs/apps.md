# Applications

`winarchy-apps.exe` holds the companion applications in one executable, one
subcommand each: `shot`, `tasks`, `files`. The daemon starts them from its own
directory with the `app <name>` command, so `keybindings.toml` binds them
(`Alt+E` for `app files`, `Alt+Shift+Escape` for `app tasks`, `Win+Shift+S`
for `app shot` by default) and the `Apps` submenu of Alt+Shift+Space and the
launcher list them as Files, Tasks and Screenshot.

## Resident process

`winarchy-apps.exe serve` keeps the file manager and the task manager ready:
both windows are built once and hidden, and `app files` or `app tasks` only
shows them. The daemon starts it when it is ready, asks it over an
owner-only named pipe (`winarchy-apps-<SID>-<session>`, one instance per
user and session), and falls back to a plain `winarchy-apps.exe <name>`
process when it does not answer, restarting it for the next time. `q` hides
the window and keeps its state (directory, sort, filter); the next show reads
the theme again. Showing a window takes 10 to 60 ms in the resident, against
0.5 s for a fresh process and 2 to 3 s when Defender has not yet scanned a
newly installed executable. The screenshot tool also uses the resident: a dedicated
thread prepares its window class, cursor and message queue at startup, without
showing an overlay or capturing the desktop. `app shot` captures the current
screen on demand; repeated requests during a selection are ignored. Completing
or cancelling a selection releases its screen buffers and keeps the worker ready.
A failed resident request falls back to the standalone screenshot process.
`winarchy-apps.log` records the capture-to-overlay preparation time in milliseconds.
The resident uses 25 to 45 MB and samples processes only while Tasks is
shown; `winarchyctl quit` stops it with the daemon.

They have no title bar (the manager's border marks the focused one), read the
active theme from the configuration home when they are shown and follow theme
and shared `background_opacity` changes live, including while hidden. Their
backgrounds default to 85% opacity; text and selection stay readable and opaque.
Ctrl+Alt+Shift+Y/U adjust opacity by five points for all companion windows (see
[themes](themes.md#adjust-opacity-live)). They are otherwise
ordinary windows: Winarchy tiles them like any client, and a
rule on `winarchy-apps.exe` in `rules.toml` can float them instead. Their
Slint markup is compiled into the executable; the interpreter is reserved
for applets. A crash in an application never touches the window manager, and
the process holding the keyboard hook contains no file or process code.

In every application `q` quits, `?` shows the keys, and the bottom line shows
the mode, a pending question or the last error. Failures are also appended to
`winarchy-apps.log` in the temporary directory.

## Tasks

One list: PID, name, CPU percent of all logical processors over the last two
seconds, and working set. Protected processes that cannot be opened appear
dimmed, without figures, and cannot be ended.

| Keys | Action |
|---|---|
| `j` / `k`, arrows, PageUp / PageDown, `gg` / `G` | Move |
| `/` | Filter by name (smart case: lower-case matches any case); Enter keeps it, Escape clears it |
| `s` | Cycle the sort: cpu, memory, name, pid |
| `x` | End the process under the cursor, after `y` |
| `?` / `q` / Escape | Help / quit |

The cursor follows its process across refreshes rather than its row.

## Files

Three columns after yazi: the parent directory, the current directory and a
preview of the entry under the cursor (a directory listing, the first lines of
a text file, or size, type and age for anything else). Directories come
first, in natural order (`file2` before `file10`), hidden entries are hidden
until `.` shows them. Going up from a drive root lists the drives and the WSL
distributions (`wsl: Debian`, reached through `\\wsl.localhost\`, read from
the registry since that share cannot be enumerated). Coming back to a
directory puts the cursor where it was.

| Keys | Action |
|---|---|
| `h` / `l`, Left / Right, Backspace / Enter | Parent / enter the directory or open the file with its default application |
| `j` / `k`, arrows, `gg` / `G`, Ctrl+D / Ctrl+U, PageUp / PageDown | Move |
| Space | Toggle the selection of the entry and move down |
| `v` | Visual mode: the selection follows the cursor from the anchor; `v` or Escape ends it |
| Ctrl+A | Select everything shown |
| Escape | Clear the selection |
| `y` / `x` | Yank / cut the selection, or the entry under the cursor |
| `p` | Paste into the current directory; yanked paths stay available |
| `d` / `D` | Move to the recycle bin / delete permanently, after `y` |
| `r` | Rename the entry under the cursor |
| `a` | Create; a trailing `/` makes a directory |
| `.` | Toggle hidden files |
| `/` | Filter; Enter keeps it, Escape clears it |
| `s` | Cycle the sort: name, size, modified |
| `o` | Open the `terminal` alias of `apps.toml` in the current directory |
| `~` | Home directory |
| `w` | Home of the default user in the default WSL distribution (`DefaultUid` and its `/etc/passwd`) |
| `?` / `q` | Help / quit |

Copy, move and delete run through the shell's file operation service, so
the recycle bin, progress and name-conflict dialogs are the system's own.
Rename and create use plain file system calls. Listings stop at 10,000
entries with a notice. Nothing is previewed as an image, and there are no
tabs, bookmarks or plugins.

`winarchy-apps.exe files <directory>` starts in that directory instead of
the user profile; through the resident, `winarchyctl app files` shows the
directory left open, or the profile the first time.

## Opacity desktop smoke test

On an unlocked Windows desktop, with test windows unobscured:

```powershell
./scripts/test-apps-opacity-desktop.ps1 -Executable C:\path\to\winarchy-apps.exe
```

This opens disposable standalone Files and Tasks windows and checks live opacity,
reset, palette reload, invalid-theme retention and unchanged window handles. It
never writes personal configuration or injects global keys. Also manually check
resident windows hidden/reopened, readable text and selection, and multiple
terminal/browser windows sharing the daemon's shortcuts.

## Screenshot

`shot` freezes the virtual screen dimmed; drag a rectangle to copy it to the
clipboard as a bitmap. Escape or a right click cancels.

Manual Windows check after restarting Winarchy: confirm no overlay appears at
startup, then time the first Win+Shift+S and subsequent invocations. Compare the
`shot: overlay ready` log entries (capture/preparation only, not process startup).
Test Escape, right click, focus loss, repeated shortcuts during selection and
clipboard paste after confirmation. Change monitor layout/DPI and repeat; the
next capture must reflect the new desktop. Repeated captures should not grow
GDI handles or retained screen buffers, and Files/Tasks must still work after
cancellation. Stop the resident and check that the next shortcut still works
through the standalone fallback.
