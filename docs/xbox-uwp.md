# Horizon on the Xbox (UWP, Dev Mode)

**Status: prototype, first steps confirmed on an Xbox.** A package without the game files
installed on a console in Dev Mode, started, and drew Horizon's no-resources screen full screen
with DirectX 12 (November 2026): the window, the swapchain, the renderer and the app's folders
work there. Nobody has played a world on it yet. This page says what is there, how to build and
install it, and what to check next.

The Xbox's Dev Mode runs UWP apps you sideload yourself, on your own console, for development
only. A package built here holds the game's prepared resources, which come from your own Minecraft
install: **never share it** and never commit it (`tools/check_public_tree.py`).

## What the port changes

| Part | Desktop | UWP |
|---|---|---|
| Window and input | winit's Win32 backend | `third_party/winit`'s UWP backend (`src/platform_impl/uwp`): `CoreApplication::Run`, the one `CoreWindow`, its key, text, pointer, wheel and raw-mouse events; the Xbox's back navigation (B) marked handled; layout scaling off, so the window is 1920 × 1080 real pixels |
| Swapchain | DXGI on the window's `HWND` | `CreateSwapChainForCoreWindow` (`third_party/wgpu-hal`, `SurfaceTarget::CoreWindow`) |
| Graphics API | Vulkan, DirectX 12, OpenGL | DirectX 12 only (`gfx.rs` `backend_order`; no `gles` feature on UWP) |
| Boot screen | painted with GDI before the first frame | none (`horizon-threads` `boot_paint` is desktop only) |
| Clipboard | the system's (arboard) | the game's own: text copied in the game pastes in the game |
| Opening links and folders | the shell (`open`) | refused: there is no desktop (`desktop.rs`) |
| Folders | working directory, `%APPDATA%\horizon` | the app's `LocalState` (`horizon-threads/src/uwp.rs`): the home (logs, crash reports) is `LocalState`, saves and `options.txt` in `LocalState\game` |
| Resources | `--resources` or `resources\` beside the exe | `resources\` in the package, beside the exe, else `LocalState\resources`, unpacked from an uploaded `resources.zip` (`resources_zip.rs`) |
| Executable | console subsystem | Windows subsystem (`packaging/windows/exe.rs`) |

Controllers need nothing new: the game reads them through `Windows.Gaming.Input`, the Xbox's own
API. The backend drops the gamepad keys a UWP window also receives, so a press counts once.

## Build and package (on Windows)

You need:

- Visual Studio 2022 with the **Universal Windows Platform development** workload (the UWP C
  runtime, the Windows SDK, `makeappx` and `signtool`).
- Nightly Rust with the standard library's source, since the target has no prebuilt one:
  `rustup toolchain install nightly --component rust-src`.
- The prepared resources (`.local/horizon-client/26.3`, [native-client.md](native-client.md#run-from-a-checkout)).
- A signing certificate whose subject is the manifest's publisher, `CN=HorizonDev` by default. In
  PowerShell:

  ```powershell
  $cert = New-SelfSignedCertificate -Type Custom -Subject "CN=HorizonDev" -KeyUsage DigitalSignature `
    -CertStoreLocation Cert:\CurrentUser\My -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
  Export-PfxCertificate -Cert $cert -FilePath .local\uwp\dev.pfx -Password (Read-Host -AsSecureString)
  Export-Certificate -Cert $cert -FilePath .local\uwp\dev.cer
  ```

Then, from a Developer PowerShell for Visual Studio (it puts the UWP libraries on the linker's
path):

```powershell
python tools/uwp_package.py --pfx .local\uwp\dev.pfx --pfx-password <password>
```

It builds `horizon-client` for the UWP target with the `fast` profile, lays the package out in
`.local\uwp\layout` (the exe, `resources\`, the tile images made from Horizon's icon, the manifest
from `tools/uwp/AppxManifest.xml`), packs `.local\uwp\Horizon_0.1.0.0_x64.appx` and signs it.
`--dxc <folder>` also ships `dxcompiler.dll` and `dxil.dll`; without them DirectX 12 compiles
shaders with FXC, which is part of Windows.

### Without Windows

The same package can be built on Linux, as it was for the first console test:

- [xwin](https://github.com/Jake-Shadle/xwin) for the MSVC C runtime and the Windows SDK (it asks you
  to accept Microsoft's license), plus the C runtime's **Store** libraries (`lib\x64\store`, the
  `Microsoft.VC.14.xx.CRT.x64.Store.base` package of the same Visual Studio manifest), which xwin
  doesn't fetch.
- `clang-cl` for the C and C++ parts, with xwin's `crt/include` and `sdk/include/{ucrt,um,shared,winrt}`
  passed as `/imsvc`; with clang older than 19, also `-D_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH`.
- `lld-link` as the linker, with `/LIBPATH:` the Store libraries first, then xwin's
  `sdk/lib/ucrt/x86_64` and `sdk/lib/um/x86_64`.
- Microsoft's [makemsix](https://github.com/microsoft/msix-packaging) (built with `-DMSIX_PACK=on`) and
  `osslsigncode` to pack and sign: `tools/uwp_package.py --no-build --makemsix <path> --pfx <pfx>`.

`--no-resources` leaves the game's files out: the game then opens its no-resources screen, which is
enough to check that it starts, opens its window and draws on the console.

## Install on the console

1. Turn the console into Dev Mode (the Xbox Dev Mode Activation app and a Partner Center
   developer account) and open its **Device Portal** from a browser on the same network.
2. **Add** the app: the `.appx`, the certificate (`dev.cer`), and the C runtime it depends on,
   `Microsoft.VCLibs.x64.14.00.appx` from Visual Studio (`C:\Program Files (x86)\Microsoft
   SDKs\Windows Kits\10\ExtensionSDKs\Microsoft.VCLibs\14.0\Appx\Retail\x64`).
3. In Dev Home, open the app's **View details** and set its type to **Game**. An app gets a
   small share of the console's memory and GPU; a game gets far more.
4. The game makes its folders in `LocalState` (Device Portal's file explorer:
   `LocalAppData\HorizonDev.Horizon_…\LocalState`): logs and crash reports there, saves in
   `game\`.

### The game files on their own

A package built with `--no-resources` (or without resources in it) looks for the prepared files
in `LocalState\resources` instead, so a new build doesn't carry them again:

1. On the computer that prepared them: `python tools/uwp_package.py --zip-resources` writes
   `.local\uwp\resources.zip` (`--resources <folder>` for another folder, such as the launcher's).
2. In Device Portal's file explorer, open the app's `LocalState` and upload `resources.zip`. It
   is one file, so a phone's browser can do it too.
3. Start Horizon. Its no-resources screen unpacks the zip (the window is already up: a UWP app
   that takes too long to show it at launch is ended), shows how many files are out, then asks
   for a restart. The zip is deleted once unpacked; a failed or stopped unpacking leaves it to
   try again.

Like the package, the zip holds files from your own Minecraft install: never share it.

## What to check first on the console

In order, since each needs the one before:

1. **It starts** (confirmed). Rust's standard library on this target still imports a few functions outside
   the official UWP list: `ProcessPrng` (`bcryptprimitives.dll`, the random seed of every
   `HashMap`), `QueryDosDeviceW` and the `ntdll` file calls behind `std::fs`. Those DLLs exist in
   every Windows process, so the package should load, but only the console can say it does. A
   crash at once is most likely here.
2. **It draws** (confirmed on the no-resources screen). The title screen at 1920 × 1080: the CoreWindow swapchain and DirectX 12 on the
   console's GPU.
3. **A controller plays it.** Menus and movement with the pad; B must not leave the app.
4. **Sound.** The audio output goes through cpal's WASAPI host, which Microsoft lists for UWP apps
   but nobody has tried there. If it fails, the game plays silent; an XAudio2 output is the fix.
5. **A world saves and loads again** after quitting from the pause menu.
6. **Multiplayer.** Joining a server should work (Winsock). Opening a world to LAN uses mio's
   `\Device\Afd` polling, which the app sandbox may refuse.

## Known gaps

- **Suspend and resume.** The game does not save when the console suspends it (Home button, or
  the console going to sleep), and the console may end a suspended game. Save from the pause
  menu before leaving.
- **Text input.** No on-screen keyboard is asked for: chat and signs need a USB keyboard.
- **Window placement, monitors, fullscreen switches and the cursor's look**: a UWP window has
  none of these to change. The cursor can only be shown or hidden.
- **Xbox One** consoles have far less GPU and memory than Series X|S; lower the render distance.
- **Unit tests** for the backend's key and text handling only build for the UWP target, so they
  can't run in CI or on Linux.

## How the import check was done

On Linux, without Microsoft's tools: the game built for the UWP target with nightly
`-Zbuild-std`, its C and C++ parts compiled with clang against the mingw-w64 headers, linked with
`lld-link` against mingw-w64's import libraries. The executable's import table was then compared
with `libwindowsapp.a` (mingw-w64's list of the functions UWP apps may call) and, for the DLLs that
list leaves out (Winsock, DXGI, Direct3D 12, BCrypt), Microsoft's documented UWP API list. 249
functions are imported; the 13 outside both lists are the standard library's, `fs4`'s and mio's
calls named above.
