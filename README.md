Rust implementation of indexing node for [ergvein][https://github.com/hexresearch/ergvein] mobile wallet.

# How to build

1. The local nix shell provides [rustup](https://rustup.rs/) or install it manually.
2. Add toolchain: `rustup toolchain add nightly && rustup default nightly`
3. You will need `clang` and `llvm` to build rocksdb dependency.
3. Build: `cargo build` and `cargo build --release` for release binary.

# How to run
You will need either any public remote node, or local one (prefer for speed of indexation):
```
cargo run --release -- 127.0.0.1:8333
```
