//! Print the linked FFmpeg version — a smoke test that the static libraries are wired up.
//!
//! Run with: `cargo run --example version`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Linked FFmpeg version: {}", media::version_info());
    Ok(())
}
