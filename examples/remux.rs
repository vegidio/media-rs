//! Change a file's container without touching the encoded data: a stream-copy remux.
//!
//! Shows: `MediaWriter::add_stream_copy` to pass every stream (video + audio) through
//! verbatim — no decode, no re-encode — remapping each packet's stream index onto the new
//! output. Far faster and lossless compared to a full transcode.
//!
//! Run with: `cargo run --example remux`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video2.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_remux.mkv");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let mut writer = MediaWriter::create(OUTPUT)?;

    // Add one copied output stream per input stream, remembering the input -> output mapping.
    let mut out_index = Vec::with_capacity(reader.stream_count());
    for i in 0..reader.stream_count() {
        out_index.push(writer.add_stream_copy(&reader, i)?);
    }

    writer.write_header()?;

    // Copy packets untouched. `write_packet` picks the output stream from the packet's index
    // and rescales its timestamps into that stream's time base, so remapping the index is all
    // we need — the PTS/DTS stay as the demuxer produced them.
    let mut packets = 0_u64;
    for packet in reader.packets() {
        let mut packet = packet?;
        packet.set_stream_index(out_index[packet.stream_index()]);
        writer.write_packet(&mut packet)?;
        packets += 1;
    }

    writer.write_trailer()?;

    println!("Remuxed {packets} packets into {OUTPUT} (no re-encoding)");

    // Confirm the container now holds the same streams, still decodable.
    let info = probe(OUTPUT)?;
    println!("Verified: {} streams, {:.2}s", info.stream_count(), info.duration().as_secs_f64());

    for stream in info.streams() {
        println!("  stream {} — {:?}", stream.index, stream.kind);
    }

    Ok(())
}
