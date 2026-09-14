# Applications

`winarchy-apps.exe` holds the companion applications in one executable, one
subcommand each: `shot`, `tasks`, `files`. The daemon starts them from its own
directory with the `app <name>` command, so `keybindings.toml` binds them
(`Alt+E` for `app files`, `Alt+Shift+Escape` for `app tasks`, `Win+Shift+S`
for `app shot` by default) and the `Apps` submenu of Alt+Shift+Space and the
launcher list them as Files, Tasks and Screenshot.

They read the active theme from the configuration home when they start and
are otherwise ordinary windows: Winarchy tiles them like any client, and a
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
until `.` shows them. Going up from a drive root lists the drives. Coming back
to a directory puts the cursor where it was.

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
| `?` / `q` | Help / quit |

Copy, move and delete run through the shell's file operation service, so
the recycle bin, progress and name-conflict dialogs are the system's own.
Rename and create use plain file system calls. Listings stop at 10,000
entries with a notice. Nothing is previewed as an image, and there are no
tabs, bookmarks or plugins.

`winarchy-apps.exe files <directory>` starts in that directory instead of
the user profile.

## Screenshot

`shot` freezes the virtual screen dimmed; drag a rectangle to copy it to the
clipboard as a bitmap. Escape or a right click cancels.
