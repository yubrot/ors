#!/bin/sh -e

if [ $# -lt 3 ]; then
  echo "Usage: $0 <DISK_IMG> <MOUNT_POINT> <BOOTLOADER_EFI> [<KERNEL_ELF>]"
  exit 1
fi

DEVENV_DIR=$(dirname "$0")
DISK_IMG=$1
MOUNT_POINT=$2
BOOTLOADER_EFI=$3
KERNEL_ELF=$4

if [ ! -f $BOOTLOADER_EFI ]; then
  echo "No such file: $BOOTLOADER_EFI"
  exit 1
fi

# Create a disk image and format it to FAT
rm -f $DISK_IMG
qemu-img create -f raw $DISK_IMG 200M
mkfs.fat -n 'ORS' -s 2 -f 2 -R 32 -F 32 $DISK_IMG

# Initialize disk image
mkdir -p $MOUNT_POINT
sudo mount -o loop $DISK_IMG $MOUNT_POINT
sudo mkdir -p $MOUNT_POINT/EFI/BOOT
sudo cp $BOOTLOADER_EFI $MOUNT_POINT/EFI/BOOT/BOOTX64.EFI
if [ "$KERNEL_ELF" != "" ]; then
  sudo cp $KERNEL_ELF $MOUNT_POINT/ors-kernel.elf
fi
sleep 0.5
sudo umount $MOUNT_POINT

