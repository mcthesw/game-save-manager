fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        println!(
            "{}\t{:016x}\t{path}",
            bytes.len(),
            xxhash_rust::xxh3::xxh3_64(&bytes)
        );
    }
}
