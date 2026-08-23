fn main() {
    if let Err(error) = jungle_cli::run() {
        eprintln!("jungle-cli: {error}");
        std::process::exit(1);
    }
}
