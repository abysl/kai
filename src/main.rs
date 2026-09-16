fn main() {
    kai::app::main();
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(kai::autoplay::exit_code());
}
