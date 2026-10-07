# Filters

**Use when:** you want to transform video during a transcode — resize, force a frame rate,
denoise, adjust color. `media-rs` gives you a typed, composable
[`VideoFilterChain`](../reference/filter.md#videofilterchain) so you rarely have to write raw
libavfilter strings, with a `raw` escape hatch for everything else.

## Building a chain

Each operator appends a stage; stages run in the order you add them:

```rust
use media::prelude::*;

let chain = VideoFilterChain::new()             // (1)!
    .scale(640, 360)                            // (2)!
    .fps(24)                                    // (3)!
    .denoise(DenoiseLevel::Moderate)            // (4)!
    .color_correct(|cc| {                       // (5)!
        cc.brightness(0.05).contrast(1.1).saturation(1.2)
    });

println!("filter graph: {}", chain.description()); // (6)!
# Ok::<(), media::Error>(())
```

1. `VideoFilterChain::new()` starts an empty chain (a no-op until you add stages).
2. `scale(w, h)` resizes every frame. This is separate from `VideoConfig::resolution` — use
   whichever fits; setting a resolution on the encoder inserts a scale for you.
3. `fps(24)` forces a constant output frame rate, duplicating or dropping frames as needed.
4. `denoise` takes a [`DenoiseLevel`](../reference/filter.md#denoiselevel) —
   `Light` / `Moderate` / `Heavy` — mapping to tuned `hqdn3d` settings.
5. `color_correct` takes a closure over a [`ColorCorrect`](../reference/filter.md#colorcorrect)
   builder. Only the knobs you touch move away from their identity defaults (brightness `0.0`,
   contrast/saturation/gamma `1.0`).
6. `description()` returns the generated libavfilter string — here
   `scale=640:360,fps=24,hqdn3d=4:4:9:9,eq=brightness=0.05:contrast=1.1:saturation=1.2:gamma=1`.
   Handy for logging or debugging.

## Applying it in a transcode

Pass the chain to `.video_filter(...)` on any transcode tier:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let chain = VideoFilterChain::new().scale(640, 360).fps(24).denoise(DenoiseLevel::Light);

let summary = transcode("input.mp4")
    .to("output.mp4")
    .drop_audio()
    .video_filter(chain)   // (1)!
    .run()?;
# Ok(()) }
```

1. The chain is applied to the video stream before encoding. Combine it freely with
   `.trim(...)`, `.video(...)`, etc.

## The raw escape hatch

For anything the typed builders don't cover, pass a libavfilter string straight through:

```rust
use media::prelude::*;

// Scale, then sharpen with unsharp — unsharp has no typed builder, so use `raw`.
let chain = VideoFilterChain::raw("scale=1280:720,unsharp=5:5:1.0"); // (1)!
# let _ = chain;
```

1. `VideoFilterChain::raw(...)` uses the string verbatim as the entire graph. You're responsible
   for its correctness — it's passed to FFmpeg as-is.

!!! tip "Typed and raw don't mix in one chain"
    A chain is either built from typed operators **or** created from a raw string. To combine
    custom filters with typed ones, write the whole graph as a single `raw` string.

## Running a chain yourself

In a [low-level pipeline](low-level.md) there's no transcode to hand the chain to. Build it into a
[`VideoFilter`](../reference/filter.md#videofilter) for your decoder instead, and push frames
through it between decoding and encoding:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let mut reader = MediaReader::open("input.mp4")?;
let index = reader.best_stream(StreamKind::Video)?;
let time_base = reader.stream_time_base(index)?;
let mut decoder = reader.stream(index).decoder()?;

let chain = VideoFilterChain::raw("scale=1280:720,format=yuv420p");   // (1)!
let mut filter = VideoFilter::new(&decoder, time_base, &chain)?;      // (2)!
println!("out: {}x{}", filter.output_width(), filter.output_height()); // (3)!

for packet in reader.packets() {
    let packet = packet?;
    if packet.stream_index() != index { continue; }
    let frames: Vec<Frame> = decoder.decode(&packet)?.collect::<media::Result<_>>()?;
    for frame in frames {
        for scaled in filter.filter(frame)? {                          // (4)!
            let _ = scaled; // …encode it…
        }
    }
}
for scaled in filter.flush()? {                                        // (5)!
    let _ = scaled;
}
# Ok(()) }
```

1. Any chain works, typed or raw. `format=yuv420p` converts to the pixel format most encoders
   want, whatever the source's.
2. `VideoFilter::new` takes the shape of the frames from the decoder: their size, pixel format and
   sample aspect ratio. The time base is the one their timestamps are in, usually the stream's.
3. The output shape is known before any frame goes in, so you can size the encoder from it.
4. `filter` returns every frame that comes out for the one that went in: usually exactly one, but
   none or several for stages such as `fps`.
5. At the end of the stream, `flush` returns any frames the chain is still holding.

Audio works the same way, with an [`AudioFilter`](../reference/audio.md#audiofilter) built from an
`AudioFilterChain`. One use only it can do is cutting at an exact sample: skipping whole decoded
frames can only cut at frame boundaries, which can be 85 ms apart for FLAC.

```rust
use media::prelude::*;
# fn demo(decoder: &Decoder, time_base: Rational) -> media::Result<()> {
// Keep the audio from 10.5 s on. `atrim` reads each frame's pts, so set it from
// `best_effort_timestamp` before pushing the frame in.
let mut trim = AudioFilter::new(decoder, time_base, &AudioFilterChain::raw("atrim=start=10.5"))?;
# let _ = &mut trim; Ok(()) }
```

## Color correction reference

| Knob         | Identity | Meaning                                |
|--------------|:--------:|----------------------------------------|
| `brightness` |  `0.0`   | Additive shift, roughly `[-1.0, 1.0]`. |
| `contrast`   |  `1.0`   | Multiplier.                            |
| `saturation` |  `1.0`   | Multiplier (`0.0` = grayscale).        |
| `gamma`      |  `1.0`   | Gamma correction.                      |

## See also

- [Filter API reference](../reference/filter.md)
- [Transcoding](transcoding.md) — where filters are applied
