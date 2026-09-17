# Hold-to-talk dictation (experimental)

`winarchy-dictate.exe` is an optional companion executable: hold a bound key,
speak, release, and the words are pasted where the caret is. Everything runs
on this machine. There is no toggle mode, no model choice, no settings file
and no history in this version.

## Enable it

Keep `winarchy-dictate.exe` beside `winarchy.exe`, then bind the one command
in `keybindings.toml`:

```toml
"F8" = "dictate"
```

F1 to F12 may be bound alone; any other chord works too. The daemon swallows
that key entirely while the binding exists: applications no longer see it.
Reload the configuration. The daemon starts the resident (`winarchy-dictate.exe
serve`) whenever a `dictate` binding exists, and stops it with `winarchyctl quit`.
Without the executable the binding logs a warning and does nothing else;
Winarchy itself never depends on it.

The resident starts small and loads the model only when asked: on the first
press, or through `winarchy-dictate.exe --load`. Loaded, it holds about 700 MB
of RAM; `--unload` frees it again, and `--status` reports `loaded` or `idle`
through its exit code (0 or 2, anything else meaning the resident is not
running). The [Dictation applet](https://github.com/YannickHerrero/winarchy-applet-collection)
of the applet collection puts that switch in the bar.

The first load downloads the model, **Parakeet TDT 0.6B v3 (int8, about 456 MB)**,
from `blob.handy.computer` with the `curl.exe` and `tar.exe` shipped with Windows,
verifies its SHA-256 against the value pinned in the source and unpacks it under
`%LOCALAPPDATA%\Winarchy\models\`. Nothing else is ever sent or fetched. The
indicator shows the download; a mismatching checksum aborts and deletes the file.

## What happens on a press

1. Key down: the default Windows microphone opens (WASAPI shared mode, converted
   to 16 kHz mono by Windows) and a pill appears under the top of the work area
   of the monitor holding the cursor: red dot, `Listening…`, a level bar.
2. Key up: the microphone closes. Holds shorter than 300 ms are discarded.
   Longer than 60 s keep only the first minute.
3. `Transcribing…` while Parakeet runs on the CPU. Measured on the development
   machine: about 1.4 s for 8 s of French speech, and 3 s of `Loading the model…`
   the first time when the model was not in memory. It stays loaded afterwards
   until `--unload`.
4. The text goes to the clipboard, an injected Ctrl+V pastes it, and the
   previous clipboard **text** is restored 300 ms later unless the clipboard
   changed meanwhile. Other clipboard formats (images, files) are not restored.
   Fields that refuse Ctrl+V do not receive the text.

An empty transcription, a missing microphone, a failed download or a blocked
paste show a short notice in the pill instead of the text. Nothing is retried.

Parakeet v3 detects the language on its own among 25 European languages; there
is no way to force one. Punctuation and capitalization come from the model.

## Repeated presses, focus and the indicator

The pill is a topmost tool window: Winarchy does not tile it, it never takes
focus and clicks go through nothing (it has no controls). Its colors follow the
active theme when it is shown: surface, overlay, text, and the theme's red,
yellow and accent for the states. Pressing again while a transcription is
running is ignored until it finishes. `winarchyctl dictate` toggles a recording
from the command line for testing.

Diagnostics (durations and errors, never audio or text) go to `dictate.log` in
the configuration home, rotated at 1 MiB. `winarchy-dictate.exe --status`
reports whether the model is loaded, `--load` and `--unload` switch it, `--start`
and `--stop` drive a recording, `--quit` stops the resident.

## Building

The crate links ONNX Runtime statically: `ort-sys` downloads Microsoft's
prebuilt library at build time (about 300 MB, cached under
`~/.cache/ort.pyke.io`). From WSL that download goes through native TLS, so the
crate vendors OpenSSL for the host build script; the first build takes a few
minutes. The prebuilt runtime also links `DirectML.lib` and `PathCch.lib`,
which the xwin SDK stores lower-case: on a case-sensitive filesystem add the
links once:

```sh
cd ~/.cache/cargo-xwin/xwin/sdk/lib/um/x86_64
ln -s pathcch.lib PathCch.lib
ln -s directml.lib DirectML.lib
```

`DirectML.dll` is not needed at runtime: the CPU execution provider is the only
one used. The executable is about 22 MB.

## Not in this version

Toggle mode, model or language selection, GPU inference, custom vocabulary,
history, streaming display of partial text, LLM post-processing, and any
microphone selection beyond the Windows default input.
