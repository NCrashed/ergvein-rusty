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

# How to run with docker (or any compatible container runtime)

We have official docker image [ergvein/ergvein-index-server:rusty](https://hub.docker.com/r/ergvein/ergvein-index-server/tags?page=1&ordering=last_updated&name=rusty) built from the `Dockerfile`.

```
docker run --volume ergveindata:/data ergvein/ergvein-index-server:rusty --host 0.0.0.0 --bitcoin bitcoin-node-host:8333
```
# How to host as service on NixOS
1. Create nix module based on following code:
```
{config, lib, pkgs, ...}:
let
ergvein = (import <nixpkgs> {}).fetchFromGitHub {
  owner = "hexresearch";
  repo = "ergvein-rusty";
  rev = "c4eb3fdb19a90e0a153f97413a8ee68fd3f0efa3";
  sha256 = "19q3g9w8rk5rf4a22c1a8igxbhyh3j1zln57m6px918yfq7aa131";
};
in {
  imports = [
    "${ergvein}/nix/modules/ergvein.nix"
  ];
  config = {
    services.ergvein = {
      enable = true;
      externalAddress = "{ExternalAddress}:8667";
      metrics = true;
    };
    services.ergvein-rusty = {
      blockBatch = 100;
      maxCache = 40000000;
    };
 
   services.bitcoin.passwordHMAC = "{HMAC}";
  };
}
```
Where:
- ExternalAddress is IP of hosting machine thus an address of indexer
- HMAC credentials for bitcoin node RPC. Follow this to generate your own https://github.com/bitcoin/bitcoin/tree/master/share/rpcauth
2. Edit configuration.nix
- add created module to import
```
imports =
    [ # Include the results of the hardware scan.
      ./hardware-configuration.nix
      ./cachix.nix
      {PATH_TO_MODULE}
    ];

```
- allow indexer port (8667 by default for mainnet) in firewall
```
networking.firewall.allowedTCPPorts = [ 8667 ];
```
3 Apply changes by running ```nixos-rebuild switch``` command
