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

ors is based on [MikanOS (ゼロからの OS 自作入門)](https://www.amazon.co.jp/gp/product/B08Z3MNR9J) and [blog_os (Writing an OS in Rust)](https://os.phil-opp.com/).

|             | ors         | MikanOS     | blog_os       |
| ----------- | ----------- | ----------- | ------------- |
| Written in  | Rust        | C++         | Rust          |
| Boot by     | UEFI        | UEFI        | BIOS          |
| Graphics    | GOP by UEFI | GOP by UEFI | VGA Text Mode |
| Serial Port | 16550 UART  | -           | 16650 UART    |

## Roadmap

- [ ] Complete [ゼロからの OS 自作入門](https://www.amazon.co.jp/gp/product/B08Z3MNR9J)
- [ ] Complete [Writing an OS in Rust](https://os.phil-opp.com/)
- [ ] Try to implement TCP protocol stack
- [ ] Compare with [xv6](https://github.com/mit-pdos/xv6-public)
- [ ] Compare with POSIX
