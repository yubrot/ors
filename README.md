# ors

ors is an experimental OS implementation with Rust.

## Setup

```bash
# Rust nightly required at the moment
rustup default nightly

# Build ors-loader.efi and ors-kernel.elf
make

# Run on QEMU
make qemu
# ... is equivalent to
./qemu/make_and_run.sh \
    target/x86_64-unknown-uefi/debug/ors-loader.efi \
    target/x86_64-unknown-none-ors/debug/ors-kernel.elf
```

## Comparison

ors is based on [MikanOS](https://github.com/uchan-nos/mikanos) and [blog_os (Second Edition)](https://os.phil-opp.com/), and [xv6](https://github.com/mit-pdos/xv6-public).

|                     | ors            | MikanOS        | blog_os          | xv6           |
| ------------------- | -------------- | -------------- | ---------------- | ------------- |
| Written in          | Rust           | C++            | Rust             | C             |
| Boot by             | UEFI BIOS      | UEFI BIOS      | Legacy BIOS [^1] | Legacy BIOS   |
| Screen Rendering    | GOP by UEFI    | GOP by UEFI    | VGA Text Mode    | VGA Text Mode |
| Serial Port         | 16550 UART     | -              | 16650 UART       | 16650 UART    |
| Hardware Interrupts | APIC           | APIC           | 8259 PIC         | APIC          |
| Keyboard Support    | PS/2           | USB (xHCI)     | PS/2             | PS/2          |
| Mouse Support       | -              | USB (xHCI)     | -                | -             |
| Timers              | APIC + ACPI PM | APIC + ACPI PM | 8259 PIC         | APIC          |
| Multitasking        | Preemptive     | Preemptive     | WIP [^2]         | Preemptive    |

[^1]: [UEFI is planned](https://github.com/phil-opp/blog_os/issues/349)
[^2]: blog_os supports [Cooperative Multitasking](https://os.phil-opp.com/async-await/) at the moment

## Roadmap

- [ ] Complete [ゼロからの OS 自作入門](https://www.amazon.co.jp/gp/product/B08Z3MNR9J)
- [x] Complete [Writing an OS in Rust](https://os.phil-opp.com/) (second edition)
- [ ] Compare with [xv6](https://github.com/mit-pdos/xv6-public)
- [ ] Try to implement TCP protocol stack
- [ ] Compare with POSIX
