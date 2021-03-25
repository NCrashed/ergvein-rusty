with import ./nix/pkgs.nix {};
let merged-openssl = symlinkJoin { name = "merged-openssl"; paths = [ openssl.out openssl.dev ]; };
in stdenv.mkDerivation rec {
  name = "rust-env";
  env = buildEnv { name = name; paths = buildInputs; };

  buildInputs = [
    rustup
    clang
    llvmPackages_11.libclang
    openssl
    valgrind
    kdeApplications.kcachegrind
    llvm_11
  ];
  shellHook = ''
  export LIBCLANG_PATH="${llvmPackages.libclang}/lib"
  export OPENSSL_DIR="${merged-openssl}"
  '';
}
