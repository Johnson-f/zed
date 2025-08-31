#[cfg(feature = "fuzzing")]
fn main() {
    scheduler::fuzzing::fuzz_targets::fuzz_target_basic();
}

#[cfg(not(feature = "fuzzing"))]
fn main() {
    println!("Fuzzing not enabled. Compile with --features fuzzing");
}