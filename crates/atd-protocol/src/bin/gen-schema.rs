// Placeholder stub. The real implementation lands in T6 of SP-protocol-schema.
// Exists now only so that the `[[bin]]` entry in Cargo.toml resolves; the bin
// is gated by `required-features = ["schema"]` so default builds never compile it.

fn main() {
    eprintln!("gen-schema: placeholder — full implementation in SP-protocol-schema T6");
    std::process::exit(1);
}
