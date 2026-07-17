# Seeking

**Use when:** you want to start reading at a timestamp instead of decoding a file from the
beginning. Seeking is **keyframe-granular**: you land at or before the nearest keyframe, then
decode forward to reach the exact frame you want. This is the primitive that
[frame extraction](frame-extraction.md) and fast [trimming](transcoding.md#trimming-to-a-time-range)
are built on.

## Seek, then decode forward

```rust
use media::prelude::*;
use std::time::Duration;

let mut reader = MediaReader::open("input.mp4")?;
let vidx = reader.best_stream(StreamKind::Video)?;
let tb = reader.stream_time_base(vidx)?.as_f64();  // (1)!

let mut decoder = reader.stream(vidx).decoder()?;

reader.seek(vidx, Duration::from_secs(2))?;        // (2)!
decoder.reset();                                   // (3)!

let target = 2.0_f64;
let mut shown = 0;
'outer: for packet in reader.packets() {           // (4)!
    let packet = packet?;
    if packet.stream_index() != vidx { continue; }

    for frame in decoder.decode(&packet)? {
        let frame = frame?;
        let secs = frame.best_effort_timestamp()   // (5)!
            .map(|ts| ts as f64 * tb)
            .unwrap_or(f64::NAN);

        if secs + f64::EPSILON < target { continue; } // (6)!

        println!("frame @ {secs:.3}s");
        shown += 1;
        if shown == 5 { break 'outer; }
    }
}
# Ok::<(), media::Error>(())
```

1. The stream's **time base** converts raw timestamps to seconds. `stream_time_base` returns
   a [`Rational`](../reference/types.md#rational); `as_f64()` evaluates it. A frame's
   timestamp × time base = seconds.
2. `reader.seek(stream_index, at)` jumps so subsequent reads resume near `at`. It lands at or
   **before** the nearest keyframe — you can't seek to an arbitrary frame directly, because a
   non-keyframe can't be decoded without its predecessors.
3. **Always** `reset()` the decoder after seeking. It discards the decoder's buffered state so
   frames decoded before the jump don't leak into your output. Forgetting this is the most
   common seeking bug.
4. Read packets as usual from the new position, skipping other streams.
5. `best_effort_timestamp()` is the decoder's best guess of a frame's timestamp (preferred
   over `pts()` when you just need "where is this frame").
6. Because the seek landed on a keyframe **at or before** the target, decode forward and skip
   frames until you reach the exact time you want. This is the standard frame-accurate-seek
   pattern layered on top of a keyframe seek.

## Mental model

```
             seek(2s)
                │
   keyframe ────┼──────────────► target (2.0s)
      ▲         │       ▲
   lands here   │    decode forward, skipping,
  (≤ target)    │    until secs ≥ target
```

## Higher-level alternatives

You usually don't need to hand-roll this. Two built-ins seek for you:

- **[Frame extraction](frame-extraction.md)** — `sampled_at(...)` seeks to each sample point
  automatically.
- **[Trimming](transcoding.md#trimming-to-a-time-range)** — `.trim(start..=end)` seeks to the
  start keyframe rather than decoding and discarding the prefix.

Drop to the manual pattern above only when you need custom control over where you start.

## See also

- [Format I/O API reference](../reference/format.md) — `MediaReader::seek`
- [Codec API reference](../reference/codec.md) — `Decoder::reset`
