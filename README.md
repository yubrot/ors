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
```

## Roadmap

- [ ] Complete [ゼロからのOS自作入門](https://www.amazon.co.jp/gp/product/B08Z3MNR9)
- [ ] Try to implement TCP protocol stack
- [ ] Compare with [Writing an OS in Rust](https://os.phil-opp.com/)
- [ ] Compare with [xv6](https://github.com/mit-pdos/xv6-public)
- [ ] Compare with POSIX

