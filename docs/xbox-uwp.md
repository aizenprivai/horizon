# Horizon on the Xbox (UWP, Dev Mode)

**Status: prototype, never run on a console yet.** It builds and links for the
`x86_64-uwp-windows-msvc` target, and its import table names no Windows function a UWP app is
denied, apart from the standard library's own (below). Nobody has started it on an Xbox. This page
says what is there, how to build and install it, and what to check first on the console.

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
| Resources | `--resources` or `resources\` beside the exe | `resources\` in the package, beside the exe |
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

## Install on the console

1. Turn the console into Dev Mode (the Xbox Dev Mode Activation app and a Partner Center
   developer account) and open its **Device Portal** from a browser on the same network.
2. **Add** the app: the `.appx`, the certificate (`dev.cer`), and the C runtime it depends on,
   `Microsoft.VCLibs.x64.14.00.appx` from Visual Studio (`C:\Program Files (x86)\Microsoft
   SDKs\Windows Kits\10\ExtensionSDKs\Microsoft.VCLibs\14.0\Appx\Retail\x64`).
3. In Dev Home, open the app's **View details** and set its type to **Game**. An app gets a
   small share of the console's memory and GPU; a game gets far more.
4. Copy nothing else: the resources are in the package, and the game makes its folders in
   `LocalState` (reachable in Device Portal's file explorer, `LocalAppData\HorizonDev.Horizon_…\LocalState`).

## What to check first on the console

In order, since each needs the one before:

1. **It starts.** Rust's standard library on this target still imports a few functions outside
   the official UWP list: `ProcessPrng` (`bcryptprimitives.dll`, the random seed of every
   `HashMap`), `QueryDosDeviceW` and the `ntdll` file calls behind `std::fs`. Those DLLs exist in
   every Windows process, so the package should load, but only the console can say it does. A
   crash at once is most likely here.
2. **It draws.** The title screen at 1920 × 1080: the CoreWindow swapchain and DirectX 12 on the
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
