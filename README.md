# Horizon UWP test package (Xbox Dev Mode)

A throwaway branch: a first test of the UWP build from branch `ccr-277f58f5-jy6uwr`
(docs/xbox-uwp.md there). Delete the branch once installed.

- `Horizon_0.1.2.0_x64.appx`: the game, signed with a self-signed certificate (`CN=HorizonDev`).
  SHA-256 `527f7370cf81b42e41915dd060762ab9a82f1edca27abb2ac8a5387af703316c`.
- `dev.cer`: that certificate, so the console trusts the package.

0.1.2.0 takes the game files as one `resources.zip` uploaded to its LocalState folder: its
no-resources screen unpacks it, then asks for a restart. Install it over the earlier version
(same certificate).

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

`termux/horizon-prepare` is horizon-prepare built for ARM64 Linux (static PIE, as Android runs it, from
ccr-277f58f5-jy6uwr): it runs in Termux. `termux/prepare.sh` does the whole job there: Java 25,
Minecraft 26.3's official files with portablemc (only if you own Java Edition), the server jar,
the preparation and `resources.zip` in Downloads. In Termux (from F-Droid):

    curl -fsSL https://github.com/aizenprivai/horizon/raw/uwp-test-package/termux/prepare.sh -o prepare.sh
    bash prepare.sh

Untested on a phone so far: portablemc in Termux, and Mojang's code in the exporters on Android's
Java. The script stops at the first failure and says where.
