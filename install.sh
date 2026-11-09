#!/usr/bin/env bash
# Instala HLI en el perfil del usuario, sin sudo.
#
#   ./install.sh                 compila, instala en ~/.local/bin y ajusta el PATH
#   ./install.sh --no-path       no toca ~/.profile
#   ./install.sh --skip-build    usa target/release ya compilado
#   ./install.sh --prefix DIR    instala en DIR
#   ./install.sh --uninstall     elimina los binarios y el PATH
set -euo pipefail

SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${HOME}/.local/bin"
NO_PATH=0
SKIP_BUILD=0
UNINSTALL=0
BINS=(hli hli-tui hli-daemon hli-mcp)

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --no-path) NO_PATH=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "opción desconocida: $1" >&2; exit 2 ;;
  esac
done

path_line="export PATH=\"${PREFIX}:\$PATH\""

if [ "$UNINSTALL" = 1 ]; then
  for b in "${BINS[@]}"; do
    [ -f "${PREFIX}/${b}" ] && rm -f "${PREFIX}/${b}" && echo "eliminado ${PREFIX}/${b}"
  done
  for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
    [ -f "$rc" ] && grep -vF "$path_line" "$rc" > "$rc.tmp" 2>/dev/null && mv "$rc.tmp" "$rc" || true
  done
  echo "HLI desinstalado. HLI_HOME no se ha tocado."
  exit 0
fi

if [ ! -f "$SOURCE/Cargo.toml" ]; then
  echo "no se encontró Cargo.toml en $SOURCE" >&2
  exit 1
fi

if [ "$SKIP_BUILD" = 0 ]; then
  echo "compilando HLI en release…"
  (cd "$SOURCE" && cargo build --release --workspace)
fi

for b in "${BINS[@]}"; do
  [ -f "$SOURCE/target/release/$b" ] || { echo "falta target/release/$b" >&2; exit 1; }
done

mkdir -p "$PREFIX"
for b in "${BINS[@]}"; do
  install -m 0755 "$SOURCE/target/release/$b" "$PREFIX/$b"
  echo "instalado $PREFIX/$b"
done

if [ "$NO_PATH" = 0 ]; then
  rc="$HOME/.profile"
  if ! grep -qF "$path_line" "$rc" 2>/dev/null; then
    printf '\n# HLI\n%s\n' "$path_line" >> "$rc"
    echo "PATH: añadido a $rc"
  else
    echo "PATH: $rc ya lo contiene"
  fi
fi

"$PREFIX/hli" --version
echo
echo "Listo. Abre una terminal nueva y ejecuta: hli doctor"
