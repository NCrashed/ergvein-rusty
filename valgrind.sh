cargo build --release
valgrind --tool=callgrind target/release/ergvein-rusty
kcachegrind callgrind.out*
rm callgrind.out*
