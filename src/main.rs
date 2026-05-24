fn main() {
    if let Err(error) = aegis::run_from_env() {
        if !error.was_reported() {
            eprintln!("Aegis error: {error}");
        }
        std::process::exit(1);
    }
}
