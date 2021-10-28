#!/bin/sh -e

if [ $# -lt 1 ]; then
  echo "Usage: $0 <BOOTLOADER_EFI> [<KERNEL_ELF>]"
  exit 1
fi

DEVENV_DIR=$(dirname "$0")
DISK_IMG=./disk.img
MOUNT_POINT=./mnt
BOOTLOADER_EFI=$1
KERNEL_ELF=$2

$DEVENV_DIR/make_image.sh $DISK_IMG $MOUNT_POINT $BOOTLOADER_EFI $KERNEL_ELF
$DEVENV_DIR/run_image.sh $DISK_IMG
