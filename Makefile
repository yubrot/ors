all: ors-loader.efi ors-kernel.elf

ors-loader.efi:
	cd ors-loader && cargo build

ors-kernel.elf:
	cd ors-kernel && cargo build

qemu: ors-loader.efi ors-kernel.elf
	cd ors-kernel && cargo run
