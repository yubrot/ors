#!/bin/sh -ex

if [ $# -lt 3 ]; then
  echo "Usage: $0 <image name> <mount point> <.efi file> [another file]"
  exit 1
fi

DEVENV_DIR=$(dirname "$0")
DISK_IMG=$1
MOUNT_POINT=$2
EFI_FILE=$3
ANOTHER_FILE=$4

if [ ! -f $EFI_FILE ]; then
  echo "No such file: $EFI_FILE"
  exit 1
fi

# Create a disk image and format it to FAT
rm -f $DISK_IMG
qemu-img create -f raw $DISK_IMG 200M
mkfs.fat -n 'ors' -s 2 -f 2 -R 32 -F 32 $DISK_IMG

# Initialize disk image
mkdir -p $MOUNT_POINT
sudo mount -o loop $DISK_IMG $MOUNT_POINT
sudo mkdir -p $MOUNT_POINT/EFI/BOOT
sudo cp $EFI_FILE $MOUNT_POINT/EFI/BOOT/BOOTX64.EFI
if [ "$ANOTHER_FILE" != "" ]; then
  sudo cp $ANOTHER_FILE $MOUNT_POINT/
fi
sleep 0.5
sudo umount $MOUNT_POINT

