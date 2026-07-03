#!/usr/bin/env bash
# Assemble a bootable zeroxos ISO using the Limine bootloader.
#
# The resulting `zeroxos.iso` boots in QEMU and on real UEFI + BIOS hardware
# (flash it to a USB stick with `dd`). Requires: xorriso, git, make, a C
# compiler (all installed by `docs/MANUAL.md` prerequisites).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LIMINE_BRANCH="v9.x-binary"
LIMINE_DIR="build/limine"
ISO_ROOT="build/iso_root"
KERNEL="target/x86_64-unknown-zeroxos/release/zeroxos-boot"
OUT="zeroxos.iso"

if [[ ! -f "$KERNEL" ]]; then
    echo "[mk-iso] kernel image not found — run 'make build-x86_64' first." >&2
    exit 1
fi

# 1. Fetch + build the Limine bootloader (host install tool) if missing.
if [[ ! -x "$LIMINE_DIR/limine" ]]; then
    echo "[mk-iso] fetching Limine ($LIMINE_BRANCH)..."
    rm -rf "$LIMINE_DIR"
    git clone https://github.com/limine-bootloader/limine.git \
        --branch="$LIMINE_BRANCH" --depth=1 "$LIMINE_DIR"
    make -C "$LIMINE_DIR"
fi

# 2. Lay out the ISO tree.
echo "[mk-iso] assembling ISO tree..."
rm -rf "$ISO_ROOT"
mkdir -p "$ISO_ROOT/boot/limine" "$ISO_ROOT/EFI/BOOT"
cp "$KERNEL" "$ISO_ROOT/boot/zeroxos"
cp boot/limine.conf "$ISO_ROOT/boot/limine/limine.conf"
cp "$LIMINE_DIR/limine-bios.sys" \
   "$LIMINE_DIR/limine-bios-cd.bin" \
   "$LIMINE_DIR/limine-uefi-cd.bin" "$ISO_ROOT/boot/limine/"
cp "$LIMINE_DIR/BOOTX64.EFI" "$LIMINE_DIR/BOOTIA32.EFI" "$ISO_ROOT/EFI/BOOT/"

# 3. Build the hybrid (BIOS + UEFI) ISO.
echo "[mk-iso] building $OUT..."
xorriso -as mkisofs -R -r -J \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    -hfsplus -apm-block-size 2048 \
    --efi-boot boot/limine/limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    "$ISO_ROOT" -o "$OUT" 2>/dev/null

# 4. Install the Limine BIOS stage into the ISO.
"$LIMINE_DIR/limine" bios-install "$OUT"

echo "[mk-iso] done: $OUT"
