fn main() {
    if let Err(error) = aegis::run_from_env() {
        eprintln!("Aegis error: {error}");
        std::process::exit(1);
    }
}
