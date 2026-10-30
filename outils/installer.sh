#!/bin/sh
# Installe Vigie pour l'utilisateur courant, sans droits d'administrateur, sous
# Linux et macOS. Equivalent de installer.ps1 (Windows).
#
#   sh installer.sh                 installe (ou met a jour)
#   sh installer.sh --desinstaller  retire ce que l'installation a pose
#
# Il cherche l'executable a cote de lui (l'archive de la Release : `vigie`), puis
# dans `../target/release/vigie` (depot compile), sinon il compile avec cargo.
#
#   Linux  : ~/.local/bin/vigie, une entree de menu (vigie.desktop) et son icone.
#   macOS  : ~/Applications/Vigie.app (un bundle ; Spotlight le trouve).
#
# Les donnees (memoire de la fenetre, import) restent ; `--aussi-les-donnees`
# les retire avec la desinstallation.

set -eu

ici=$(cd "$(dirname "$0")" && pwd)
systeme=$(uname -s)
maison=${HOME:?"HOME est absent"}
suppression_donnees=0
action=installer
for a in "$@"; do
    case "$a" in
        --desinstaller) action=desinstaller ;;
        --aussi-les-donnees) suppression_donnees=1 ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) echo "option inconnue : $a" >&2; exit 2 ;;
    esac
done

# --- ou sont les choses ---------------------------------------------------
if [ "$systeme" = "Darwin" ]; then
    bundle="$maison/Applications/Vigie.app"
    donnees="$maison/Library/Application Support/Vigie"
else
    binaire_installe="$maison/.local/bin/vigie"
    bureau="$maison/.local/share/applications/vigie.desktop"
    icone_installee="$maison/.local/share/icons/hicolor/256x256/apps/vigie.png"
    donnees="${XDG_DATA_HOME:-$maison/.local/share}/vigie"
fi

# --- desinstaller ---------------------------------------------------------
if [ "$action" = "desinstaller" ]; then
    if [ "$systeme" = "Darwin" ]; then
        rm -rf "$bundle"
    else
        rm -f "$binaire_installe" "$bureau" "$icone_installee"
        command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$maison/.local/share/applications" 2>/dev/null || true
    fi
    if [ "$suppression_donnees" = 1 ]; then
        rm -rf "$donnees"
    fi
    echo "Vigie est desinstallee."
    exit 0
fi

# --- trouver (ou fabriquer) l'executable -----------------------------------
if [ -x "$ici/vigie" ]; then
    source_binaire="$ici/vigie"
elif [ -x "$ici/../target/release/vigie" ]; then
    source_binaire="$ici/../target/release/vigie"
else
    command -v cargo >/dev/null 2>&1 || { echo "Pas d'executable a installer, et cargo n'est pas la pour en compiler un." >&2; exit 1; }
    (cd "$ici/.." && cargo build --release)
    source_binaire="$ici/../target/release/vigie"
fi

# --- l'icone : a cote de l'archive, ou dans le depot ------------------------
if [ -f "$ici/vigie-256.png" ]; then
    icone_source="$ici/vigie-256.png"
elif [ -f "$ici/../assets/vigie-256.png" ]; then
    icone_source="$ici/../assets/vigie-256.png"
else
    icone_source=""
fi

if [ "$systeme" = "Darwin" ]; then
    # Un bundle : l'executable, son Info.plist, son icone.
    rm -rf "$bundle"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    cp "$source_binaire" "$bundle/Contents/MacOS/vigie"
    chmod 755 "$bundle/Contents/MacOS/vigie"
    icone_plist=""
    if [ -n "$icone_source" ] && command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
        jeu=$(mktemp -d)/vigie.iconset
        mkdir -p "$jeu"
        for t in 16 32 64 128 256; do
            sips -z "$t" "$t" "$icone_source" --out "$jeu/icon_${t}x${t}.png" >/dev/null 2>&1 || true
        done
        cp "$jeu/icon_32x32.png" "$jeu/icon_16x16@2x.png" 2>/dev/null || true
        cp "$jeu/icon_64x64.png" "$jeu/icon_32x32@2x.png" 2>/dev/null || true
        cp "$jeu/icon_256x256.png" "$jeu/icon_128x128@2x.png" 2>/dev/null || true
        if iconutil -c icns "$jeu" -o "$bundle/Contents/Resources/vigie.icns" 2>/dev/null; then
            icone_plist="<key>CFBundleIconFile</key><string>vigie</string>"
        fi
        rm -rf "$(dirname "$jeu")"
    fi
    cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Vigie</string>
<key>CFBundleDisplayName</key><string>Vigie</string>
<key>CFBundleIdentifier</key><string>io.github.stoneariel491-sudo.vigie</string>
<key>CFBundleExecutable</key><string>vigie</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
$icone_plist
</dict></plist>
PLIST
    # Une application non signee, telechargee, porte la marque de quarantaine :
    # on l'ote pour qu'elle s'ouvre d'un double-clic (c'est la vigie qu'on vient d'installer soi-meme).
    xattr -dr com.apple.quarantine "$bundle" 2>/dev/null || true
    echo "Vigie installee : $bundle"
else
    mkdir -p "$(dirname "$binaire_installe")" "$(dirname "$bureau")" "$(dirname "$icone_installee")"
    cp "$source_binaire" "$binaire_installe"
    chmod 755 "$binaire_installe"
    if [ -n "$icone_source" ]; then
        cp "$icone_source" "$icone_installee"
    fi
    cat > "$bureau" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Vigie
Comment=Regarder penser les agents, en direct
Exec=$binaire_installe
Icon=vigie
Terminal=false
Categories=Development;Utility;
DESKTOP
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$maison/.local/share/applications" 2>/dev/null || true
    echo "Vigie installee : $binaire_installe"
    case ":$PATH:" in
        *":$maison/.local/bin:"*) ;;
        *) echo "Note : $maison/.local/bin n'est pas dans votre PATH ; le menu des applications la trouvera quand meme." ;;
    esac
fi
exit 0
