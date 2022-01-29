# ors

ors is an experimental x86_64 OS implementation with Rust.

## Setup

```bash
# Rust nightly required at the moment
rustup default nightly

# Build ors-loader.efi and ors-kernel.elf
make

# Run on QEMU
make qemu
# ... is equivalent to
./qemu/make_and_run_image.sh \
    target/x86_64-unknown-uefi/debug/ors-loader.efi \
    target/x86_64-unknown-none-ors/debug/ors-kernel.elf
```

## Comparison

ors is based on [MikanOS](https://github.com/uchan-nos/mikanos) and [blog_os (Second Edition)](https://os.phil-opp.com/), and [xv6](https://github.com/mit-pdos/xv6-public).

|                     | ors             | MikanOS        | blog_os          | xv6           |
| ------------------- | --------------- | -------------- | ---------------- | ------------- |
| Target              | x86_64          | x86_64         | x86_64           | x86 [^1]      |
| Written in          | Rust            | C++            | Rust             | C             |
| Boot by             | UEFI BIOS       | UEFI BIOS      | Legacy BIOS [^2] | Legacy BIOS   |
| Screen Rendering    | GOP by UEFI     | GOP by UEFI    | VGA Text Mode    | VGA Text Mode |
| Serial Port         | 16550 UART      | -              | 16650 UART       | 16650 UART    |
| Hardware Interrupts | APIC            | APIC           | 8259 PIC         | APIC          |
| Keyboard Support    | PS/2            | USB (xHCI)     | PS/2             | PS/2          |
| Mouse Support       | -               | USB (xHCI)     | -                | -             |
| Block Support       | VirtIO over PCI | - [^3]         | -                | IDE [^4]      |
| Timers              | APIC + ACPI PM  | APIC + ACPI PM | 8259 PIC         | APIC          |
| Multitasking        | Preemptive      | Preemptive     | WIP [^5]         | Preemptive    |
| File System         | FAT             | FAT            | -                | original [^6] |

[^1]: Maintaining the x86 version have stopped, and switched to the [RISC-V version](https://github.com/mit-pdos/xv6-riscv)
[^2]: [UEFI is planned](https://github.com/phil-opp/blog_os/issues/349)
[^3]: Supports only very limited reading (by UEFI Block I/O)
[^4]: [RISC-V version of xv6](https://github.com/mit-pdos/xv6-riscv) supports VirtIO over MMIO
[^5]: blog_os supports [Cooperative Multitasking](https://os.phil-opp.com/async-await/) at the moment
[^6]: Simpler but similar to modern UNIX file systems, including crash recovering

## Roadmap

- [ ] Complete [ゼロからの OS 自作入門](https://www.amazon.co.jp/gp/product/B08Z3MNR9J)
- [x] Complete [Writing an OS in Rust](https://os.phil-opp.com/) (second edition)
- [ ] Compare with [xv6](https://github.com/mit-pdos/xv6-public)
- [ ] Try to implement TCP protocol stack
- [ ] Compare with POSIX

## Resources

- ors uses [Tamzen font](https://github.com/sunaku/tamzen-font).
- ors uses [One Monokai Theme](https://github.com/azemoh/vscode-one-monokai) as a color scheme of the terminal.
