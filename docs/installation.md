# Installation

## Windows build

Requirements: Windows 11 x64, current stable Rust (edition 2024), Visual Studio Build Tools with the **Desktop development with C++** workload and Windows SDK. Installing Visual Studio alone does not install the linker. Use a Developer PowerShell or `scripts\build-windows.cmd`.

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Copy `target\release\winarchy.exe` and `winarchyctl.exe` to a directory on PATH. Defaults are embedded; the configuration directory is created on first launch. No installer, service, administrator privilege or registry change is required.

Install WezTerm separately, for example `winget install wez.wezterm`. Restart the terminal after changing PATH, or configure an absolute executable path in `apps.toml`. Winarchy does not silently download or install applications.

## Developing from WSL

The application is a Windows binary, **not** a Linux/Wayland window manager. Two build paths:

1. Copy the repository to a Windows filesystem directory and run the Windows Rust toolchain from a Developer PowerShell.
2. Cross-compile using MinGW from WSL:

```sh
sudo apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Copy both .exe files from target/x86_64-pc-windows-gnu/release to C:\Tools\Winarchy.
```

For a Windows-path configuration override from WSL:

```sh
WSLENV=WINARCHY_CONFIG_HOME \
WINARCHY_CONFIG_HOME='C:\Users\YOURNAME\winarchy-test' \
/mnt/c/Tools/Winarchy/winarchy.exe
```

Keep Linux and Windows Cargo target directories separate. Unsigned executables may need review under your organization's application-control policy. Do not disable security protections to run them.

## First session

Start `winarchy.exe` with Explorer still running. Test launcher, keyboard, IPC, and `winarchyctl quit` before replacing Explorer. Save your work: existing eligible windows are rearranged and workspace switching hides windows.

For an isolated configuration set `WINARCHY_CONFIG_HOME` to a temporary directory before launching the daemon. The CLI locates the daemon by user, not by configuration directory; only one daemon per user session is supported.

## Explorer replacement test

Read [recovery](recovery.md) first. Open Task Manager and verify you can use **Run new task**. Then:

```powershell
winarchyctl quit
winarchy.exe --replace-explorer
# To finish:
winarchyctl quit
```

The replacement path initializes the shell surfaces and recovery watchdog before stopping Explorer. Existing File Explorer windows can be lost when its process is stopped. A normal quit restores hidden managed windows and restarts Explorer if Winarchy stopped it. Do not configure this experimental build as your Windows login shell.
