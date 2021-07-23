#!/bin/sh -e

if [ $# -lt 1 ]; then
  echo "Usage: $0 <DISK_IMG>"
  exit 1
fi

DEVENV_DIR=$(dirname "$0")
DISK_IMG=$1

if [ ! -f $DISK_IMG ]; then
  echo "No such file: $DISK_IMG"
  exit 1
fi

set +e
qemu-system-x86_64 \
  -m 1G \
  -drive if=pflash,format=raw,readonly=on,file=$DEVENV_DIR/OVMF_CODE.fd \
  -drive if=pflash,format=raw,file=$DEVENV_DIR/OVMF_VARS.fd \
  -drive if=ide,index=0,media=disk,format=raw,file=$DISK_IMG \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -serial mon:stdio \
  $QEMU_OPTS
[ $? -eq 33 -o $? -eq 0 ]
