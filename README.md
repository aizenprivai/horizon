# Horizon UWP test package (Xbox Dev Mode)

A throwaway branch: a first test of the UWP build from branch `ccr-277f58f5-jy6uwr`
(docs/xbox-uwp.md there). Delete the branch once installed.

- `Horizon_0.1.3.0_x64.appx`: the game, signed with a self-signed certificate (`CN=HorizonDev`).
  SHA-256 `451633903070580db1dcf54224011e4f3fab22fb8c64b2d8a6c54b4491a1894e`.
- `dev.cer`: that certificate, so the console trusts the package.

0.1.3.0 runs the game loop on a 64 MB stack (0.1.2.0 reached the title screen, then crashed
with nothing logged) and writes a crash outside Rust's panics to LocalState\native-crash.txt.
Game files come from `resources.zip` uploaded to LocalState. Install it over the earlier
version (same certificate).

It holds **no Minecraft files**: the game opens its "no resources" screen. That is the test:
does it start, open its window and draw on the console.

## Install from a phone

1. Xbox in Dev Mode, Device Portal on (Dev Home > Remote Access settings).
2. Download both files here (open each, then "Download raw file").
3. In the phone's browser: `https://<Xbox IP>:11443` > Add > pick the `.appx`, add `dev.cer` as
   the certificate, install.
4. Dev Home > Horizon > View details > App type: **Game**. Launch it.

If the install asks for `Microsoft.VCLibs.140.00`, the console lacks the C runtime framework:
report it. If it starts and quits, the log is in Device Portal > File explorer > LocalAppData >
`HorizonDev.Horizon_…` > LocalState > `logs`.

## The game files

On a computer that prepared them (docs/xbox-uwp.md on branch `ccr-277f58f5-jy6uwr`):
`python tools/uwp_package.py --zip-resources` writes `.local/uwp/resources.zip`. Upload it with
Device Portal > File explorer > LocalAppData > `HorizonDev.Horizon_…` > LocalState, then start
Horizon. The zip holds files from your own Minecraft install: never share it.

## Preparing the game files on an Android phone (Termux)

`termux/prepare.sh` does the whole job in Termux: Java 25, Minecraft 26.3's official files with
portablemc (only if you own Java Edition), the server jar, horizon-prepare compiled there with
Termux's Rust from ccr-277f58f5-jy6uwr, the preparation and `resources.zip` in Downloads. A
static Linux build of horizon-prepare got neither its arguments nor the right to start Java
through Android's loader, so it is built natively instead. Use the script's address at a commit
(GitHub caches a branch's raw files for five minutes).

Tested on a phone so far: Java 25, portablemc installing 26.3, the server jar.
