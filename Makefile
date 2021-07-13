all: ors-loader.efi ors-kernel.elf

ors-loader.efi: FORCE
	cd ors-loader && cargo build
	cp target/x86_64-unknown-uefi/debug/ors-loader.efi ors-loader.efi

ors-kernel.elf: FORCE
	cd ors-kernel && cargo build
	cp target/x86_64-unknown-none-ors/debug/ors-kernel ors-kernel.elf

qemu: ors-loader.efi ors-kernel.elf
	./qemu/make_and_run.sh ors-loader.efi ors-kernel.elf

FORCE:
