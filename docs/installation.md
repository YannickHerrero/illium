# Installation

Build for the `x86_64-pc-windows-msvc` target. The MinGW target still compiles but is not the supported distribution route.

## Windows build

Requirements: Windows 11 x64, current stable Rust (edition 2024), Visual Studio Build Tools with the **Desktop development with C++** workload and Windows SDK. Installing Visual Studio alone does not install the linker. Use a Developer PowerShell or `scripts\build-windows.cmd`.

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Copy `target\release\illium.exe`, `illiumctl.exe` and `illium-apps.exe` to a directory on PATH; the daemon launches the companion applications from its own directory. Defaults are embedded; the configuration directory is created on first launch. No installer, service, administrator privilege or registry change is required. The corrected daemon explicitly refuses an elevated token before opening configuration or log paths.

The default alias uses WezTerm. Alternatively, opt into the bundled
[native WSL terminal](terminal.md) by setting `terminal = "illium-terminal.exe"`
in `apps.toml`; keep all Illium executables together. No separate terminal
installation is needed for that option.

[Hold-to-talk dictation](dictate.md) is another optional companion: keep
`illium-dictate.exe` beside the daemon and bind `dictate` to a key. It
downloads its speech model on first use.

For the default alias, install WezTerm separately, for example `winget install wez.wezterm`. Restart the terminal after changing PATH, or configure an absolute executable path in `apps.toml`. Illium does not silently download or install applications.

## Developing from WSL

The application is a Windows binary, **not** a Linux/Wayland window manager. Three build paths:

1. Copy the repository to a Windows filesystem directory and run the Windows Rust toolchain from a Developer PowerShell.
2. Cross-compile for the MSVC target from WSL with [cargo-xwin](https://github.com/rust-cross/cargo-xwin). It downloads the Microsoft CRT and Windows SDK headers/libraries itself, so no Visual Studio or administrator rights are needed on the Windows side:

```sh
cargo install cargo-xwin --locked
rustup target add x86_64-pc-windows-msvc
cargo xwin build --workspace --release --target x86_64-pc-windows-msvc --locked
# Copy illium.exe, illiumctl.exe, illium-apps.exe and the optional illium-terminal.exe
# and illium-dictate.exe from target/x86_64-pc-windows-msvc/release
# to %LOCALAPPDATA%\Programs\Illium.
```

`clang-cl`, `lld-link` and `llvm-rc` must be on PATH: `llvm-rc` compiles the embedded version information and manifest (`build.rs`, `illium.manifest`). Without root, download the Debian `clang-19`, `lld-19` and `llvm-19` packages with `apt-get download`, extract them with `dpkg -x` into a user prefix, and add its `bin` directory to PATH. Any `cargo clippy --target x86_64-pc-windows-msvc` from WSL needs the same PATH.

The resulting executables depend on `VCRUNTIME140.dll` and the Universal CRT, which Windows 11 and the Visual C++ Redistributable provide.

3. Cross-compile using MinGW from WSL (`x86_64-pc-windows-gnu`). This builds, but the MSVC target is the supported route:

```sh
sudo apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

For a Windows-path configuration override from WSL:

```sh
WSLENV=ILLIUM_CONFIG_HOME \
ILLIUM_CONFIG_HOME='C:\Users\YOURNAME\illium-test' \
/mnt/c/Tools/Illium/illium.exe
```

Keep Linux and Windows Cargo target directories separate. Unsigned executables may need review under your organization's application-control policy. Do not disable security protections to run them.

## First session

Start `illium.exe` with Explorer still running. Test launcher, keyboard, IPC, and `illiumctl quit` before replacing Explorer. Save your work: existing eligible windows are rearranged and workspace switching hides windows.

For an isolated configuration set `ILLIUM_CONFIG_HOME` to a temporary directory before launching the daemon. The CLI locates the daemon by user, not by configuration directory; only one daemon per user session is supported.

## Explorer replacement test

Read [recovery](recovery.md) first. Open Task Manager and verify you can use **Run new task**. Then:

```powershell
illiumctl quit
illium.exe --replace-explorer
# To finish:
illiumctl quit
```

The replacement path initializes the shell surfaces and recovery watchdog before stopping Explorer. Existing File Explorer windows can be lost when its process is stopped. A normal quit restores hidden managed windows and restarts Explorer if Illium stopped it. Do not configure this experimental build as your Windows login shell.
