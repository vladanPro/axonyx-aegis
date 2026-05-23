fn main() {
    if let Err(error) = axonyx_aegis::run_from_env() {
        eprintln!("Aegis error: {error}");
        std::process::exit(1);
    }
}
