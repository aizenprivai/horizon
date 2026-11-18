#!/data/data/com.termux/files/usr/bin/bash
# Prepares Horizon's game files on an Android phone, in Termux, and zips them as resources.zip for
# the Xbox (UWP) build: docs/xbox-uwp.md on the branch ccr-277f58f5-jy6uwr. Only for players who
# own Minecraft: Java Edition: it installs the official 26.3 files with portablemc, a command-line
# launcher, the way a launcher does. The zip holds files from your own copy: never share it.
#
#   curl -fsSL https://github.com/aizenprivai/horizon/raw/uwp-test-package/termux/prepare.sh -o prepare.sh
#   bash prepare.sh
#
# It can be run again: finished steps are kept.
set -euo pipefail

RAW=https://github.com/aizenprivai/horizon/raw/uwp-test-package/termux
REPO=https://github.com/aizenprivai/horizon.git
BRANCH=ccr-277f58f5-jy6uwr
WORK="$HOME/horizon-prepare"
MC="$WORK/minecraft"
VERSION=26.3
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
trap 'printf "\n\033[31mÉchec à l étape ci-dessus (ligne %s). Copie les dernières lignes pour Claude.\033[0m\n" "$LINENO"' ERR
mkdir -p "$WORK"

step "1/6 Paquets Termux : Java 25, Git, Python, zip"
pkg update -y
pkg install -y openjdk-25 git python zip curl
JAVA_HOME="$PREFIX/lib/jvm/java-25-openjdk"
[ -x "$JAVA_HOME/bin/javac" ] || JAVA_HOME="$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")"
export JAVA_HOME
"$JAVA_HOME/bin/java" -version

step "2/6 Minecraft Java $VERSION avec portablemc (fichiers officiels de Mojang)"
pip install --upgrade "portablemc<5"
portablemc --main-dir "$MC" start --dry --jvm "$JAVA_HOME/bin/java" "$VERSION"

step "3/6 server.jar de $VERSION (adresse lue dans le JSON officiel de la version)"
if [ ! -f "$WORK/server.jar" ]; then
  url=$(python -c 'import json,sys; print(json.load(open(sys.argv[1]))["downloads"]["server"]["url"])' \
    "$MC/versions/$VERSION/$VERSION.json")
  curl -fL -o "$WORK/server.jar.part" "$url"
  mv "$WORK/server.jar.part" "$WORK/server.jar"
fi

step "4/6 Fichiers de Horizon (research-lock.json et exporteurs Java, environ 5 Mo)"
if [ ! -d "$WORK/horizon/.git" ]; then
  git clone --depth 1 --filter=blob:none --sparse --branch "$BRANCH" "$REPO" "$WORK/horizon"
  git -C "$WORK/horizon" sparse-checkout set mods/horizon/src/test/java
fi
curl -fL -o "$WORK/horizon-prepare" "$RAW/horizon-prepare"
chmod +x "$WORK/horizon-prepare"

step "5/6 Préparation (plusieurs minutes : extraction, exporteurs Java, sons)"
cd "$WORK/horizon"
"$WORK/horizon-prepare" --prepare --complete --root . --minecraft-dir "$MC" \
  --server-jar "$WORK/server.jar" --java-home "$JAVA_HOME"

step "6/6 resources.zip"
rm -f "$WORK/resources.zip"
(cd ".local/horizon-client/$VERSION" && zip -qr "$WORK/resources.zip" .)
if [ ! -d "$HOME/storage/downloads" ]; then
  echo "Autorise l'accès au stockage dans la fenêtre d'Android, pour copier le zip dans Téléchargements."
  termux-setup-storage || true
  for _ in $(seq 30); do [ -d "$HOME/storage/downloads" ] && break; sleep 1; done
fi
if [ -d "$HOME/storage/downloads" ]; then
  cp "$WORK/resources.zip" "$HOME/storage/downloads/resources.zip"
  echo "Prêt : resources.zip est dans Téléchargements ($(du -h "$WORK/resources.zip" | cut -f1))."
else
  echo "Prêt : $WORK/resources.zip (pas d'accès au stockage : copie-le avec termux-setup-storage)."
fi
echo "Envoie-le dans LocalState de Horizon avec le Device Portal, puis lance Horizon."
