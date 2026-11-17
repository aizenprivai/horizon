# Horizon UWP test package (Xbox Dev Mode)

A throwaway branch: a first test of the UWP build from branch `ccr-277f58f5-jy6uwr`
(docs/xbox-uwp.md there). Delete the branch once installed.

- `Horizon_0.1.0.0_x64.appx`: the game, signed with a self-signed certificate (`CN=HorizonDev`).
  SHA-256 `81c4feb32042e1d505e3e08e35e2d37cf98ee4a6fc51384a4c5495906beade66`.
- `dev.cer`: that certificate, so the console trusts the package.

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
