/*
 * Umbrella header for bindgen.
 *
 * FFmpeg has no single public header, so this file gathers the public API of all
 * eight libraries for bindgen to parse. It is split into two parts:
 *
 *  1. The PORTABLE CORE — headers identical on every platform that depend on nothing
 *     outside FFmpeg.
 *
 *  2. The HARDWARE / PLATFORM-SPECIFIC headers. The release archives ship *all* of
 *     these on *every* platform, but each one `#include`s an external OS/SDK header
 *     (e.g. <d3d11.h>, <va/va.h>, <VideoToolbox/VideoToolbox.h>, <cuda.h>) that only
 *     exists on the matching platform/toolchain. Guarding on the FFmpeg header path is
 *     therefore NOT enough — we guard on the *external* dependency header. As a result
 *     this single wrapper is adaptive: on any build host bindgen binds exactly the
 *     hardware backends that host can parse. Building on macOS yields the core plus the
 *     self-contained backends, Vulkan, and VideoToolbox; per-OS CI builds pick up D3D,
 *     VAAPI, VDPAU, CUDA, QSV, OpenCL and AMF on their native runners.
 *
 * EVERY include is wrapped in `__has_include` so the wrapper degrades gracefully when a
 * header is absent — both for headers dropped between FFmpeg releases (e.g. avfft.h,
 * removed in FFmpeg 8) and for libraries a given archive does not ship (e.g. libpostproc).
 */

/* ============================ PORTABLE CORE ============================ */

/* ---- libavutil ---- */
#if __has_include(<libavutil/avutil.h>)
#include <libavutil/avutil.h>
#endif
#if __has_include(<libavutil/opt.h>)
#include <libavutil/opt.h>
#endif
#if __has_include(<libavutil/dict.h>)
#include <libavutil/dict.h>
#endif
#if __has_include(<libavutil/error.h>)
#include <libavutil/error.h>
#endif
#if __has_include(<libavutil/log.h>)
#include <libavutil/log.h>
#endif
#if __has_include(<libavutil/mem.h>)
#include <libavutil/mem.h>
#endif
#if __has_include(<libavutil/buffer.h>)
#include <libavutil/buffer.h>
#endif
#if __has_include(<libavutil/frame.h>)
#include <libavutil/frame.h>
#endif
#if __has_include(<libavutil/samplefmt.h>)
#include <libavutil/samplefmt.h>
#endif
#if __has_include(<libavutil/channel_layout.h>)
#include <libavutil/channel_layout.h>
#endif
#if __has_include(<libavutil/pixfmt.h>)
#include <libavutil/pixfmt.h>
#endif
#if __has_include(<libavutil/pixdesc.h>)
#include <libavutil/pixdesc.h>
#endif
#if __has_include(<libavutil/imgutils.h>)
#include <libavutil/imgutils.h>
#endif
#if __has_include(<libavutil/rational.h>)
#include <libavutil/rational.h>
#endif
#if __has_include(<libavutil/mathematics.h>)
#include <libavutil/mathematics.h>
#endif
#if __has_include(<libavutil/avstring.h>)
#include <libavutil/avstring.h>
#endif
#if __has_include(<libavutil/parseutils.h>)
#include <libavutil/parseutils.h>
#endif
#if __has_include(<libavutil/time.h>)
#include <libavutil/time.h>
#endif
#if __has_include(<libavutil/hash.h>)
#include <libavutil/hash.h>
#endif
#if __has_include(<libavutil/fifo.h>)
#include <libavutil/fifo.h>
#endif
#if __has_include(<libavutil/audio_fifo.h>)
#include <libavutil/audio_fifo.h>
#endif
#if __has_include(<libavutil/display.h>)
#include <libavutil/display.h>
#endif
#if __has_include(<libavutil/replaygain.h>)
#include <libavutil/replaygain.h>
#endif
#if __has_include(<libavutil/stereo3d.h>)
#include <libavutil/stereo3d.h>
#endif
#if __has_include(<libavutil/spherical.h>)
#include <libavutil/spherical.h>
#endif
#if __has_include(<libavutil/mastering_display_metadata.h>)
#include <libavutil/mastering_display_metadata.h>
#endif
#if __has_include(<libavutil/film_grain_params.h>)
#include <libavutil/film_grain_params.h>
#endif
#if __has_include(<libavutil/hdr_dynamic_metadata.h>)
#include <libavutil/hdr_dynamic_metadata.h>
#endif
#if __has_include(<libavutil/hwcontext.h>)
#include <libavutil/hwcontext.h>
#endif

/* ---- libavcodec ---- */
#if __has_include(<libavcodec/avcodec.h>)
#include <libavcodec/avcodec.h>
#endif
#if __has_include(<libavcodec/codec.h>)
#include <libavcodec/codec.h>
#endif
#if __has_include(<libavcodec/codec_desc.h>)
#include <libavcodec/codec_desc.h>
#endif
#if __has_include(<libavcodec/codec_id.h>)
#include <libavcodec/codec_id.h>
#endif
#if __has_include(<libavcodec/codec_par.h>)
#include <libavcodec/codec_par.h>
#endif
#if __has_include(<libavcodec/packet.h>)
#include <libavcodec/packet.h>
#endif
#if __has_include(<libavcodec/defs.h>)
#include <libavcodec/defs.h>
#endif
#if __has_include(<libavcodec/avdct.h>)
#include <libavcodec/avdct.h>
#endif
#if __has_include(<libavcodec/avfft.h>)
#include <libavcodec/avfft.h>
#endif
#if __has_include(<libavcodec/bsf.h>)
#include <libavcodec/bsf.h>
#endif
#if __has_include(<libavcodec/dirac.h>)
#include <libavcodec/dirac.h>
#endif
#if __has_include(<libavcodec/dv_profile.h>)
#include <libavcodec/dv_profile.h>
#endif
#if __has_include(<libavcodec/vorbis_parser.h>)
#include <libavcodec/vorbis_parser.h>
#endif
#if __has_include(<libavcodec/ac3_parser.h>)
#include <libavcodec/ac3_parser.h>
#endif
#if __has_include(<libavcodec/adts_parser.h>)
#include <libavcodec/adts_parser.h>
#endif

/* ---- libavformat ---- */
#if __has_include(<libavformat/avformat.h>)
#include <libavformat/avformat.h>
#endif
#if __has_include(<libavformat/avio.h>)
#include <libavformat/avio.h>
#endif

/* ---- libavfilter ---- */
#if __has_include(<libavfilter/avfilter.h>)
#include <libavfilter/avfilter.h>
#endif
#if __has_include(<libavfilter/buffersrc.h>)
#include <libavfilter/buffersrc.h>
#endif
#if __has_include(<libavfilter/buffersink.h>)
#include <libavfilter/buffersink.h>
#endif

/* ---- libavdevice ---- */
#if __has_include(<libavdevice/avdevice.h>)
#include <libavdevice/avdevice.h>
#endif

/* ---- libswscale ---- */
#if __has_include(<libswscale/swscale.h>)
#include <libswscale/swscale.h>
#endif

/* ---- libswresample ---- */
#if __has_include(<libswresample/swresample.h>)
#include <libswresample/swresample.h>
#endif

/* ---- libpostproc (not shipped in every build of the binaries) ---- */
#if __has_include(<libpostproc/postprocess.h>)
#include <libpostproc/postprocess.h>
#endif

/* ===================== HARDWARE / PLATFORM-SPECIFIC ===================== */
/* Each block is gated on the external SDK header it ultimately requires.    */

/* Self-contained backends — no external dependency, safe everywhere. */
#if __has_include(<libavcodec/mediacodec.h>)
#include <libavcodec/mediacodec.h>
#endif
#if __has_include(<libavutil/hwcontext_drm.h>)
#include <libavutil/hwcontext_drm.h>
#endif
#if __has_include(<libavutil/hwcontext_mediacodec.h>)
#include <libavutil/hwcontext_mediacodec.h>
#endif
#if __has_include(<libavutil/hwcontext_oh.h>)
#include <libavutil/hwcontext_oh.h>
#endif

/* Vulkan — headers are bundled in the archive, so available on all platforms. */
#if __has_include(<vulkan/vulkan.h>) && __has_include(<libavutil/hwcontext_vulkan.h>)
#include <libavutil/hwcontext_vulkan.h>
#endif

/* Apple VideoToolbox — macOS only. */
#if __has_include(<VideoToolbox/VideoToolbox.h>)
#if __has_include(<libavcodec/videotoolbox.h>)
#include <libavcodec/videotoolbox.h>
#endif
#if __has_include(<libavutil/hwcontext_videotoolbox.h>)
#include <libavutil/hwcontext_videotoolbox.h>
#endif
#endif

/* Direct3D 11 / DXVA2 / Direct3D 12 — Windows only. */
#if __has_include(<d3d11.h>)
#if __has_include(<libavcodec/d3d11va.h>)
#include <libavcodec/d3d11va.h>
#endif
#if __has_include(<libavutil/hwcontext_d3d11va.h>)
#include <libavutil/hwcontext_d3d11va.h>
#endif
#endif
#if __has_include(<d3d12.h>) && __has_include(<libavutil/hwcontext_d3d12va.h>)
#include <libavutil/hwcontext_d3d12va.h>
#endif
#if __has_include(<d3d9.h>) && __has_include(<dxva2api.h>)
#if __has_include(<libavcodec/dxva2.h>)
#include <libavcodec/dxva2.h>
#endif
#if __has_include(<libavutil/hwcontext_dxva2.h>)
#include <libavutil/hwcontext_dxva2.h>
#endif
#endif

/* VA-API — Linux (libva). */
#if __has_include(<va/va.h>) && __has_include(<libavutil/hwcontext_vaapi.h>)
#include <libavutil/hwcontext_vaapi.h>
#endif

/* VDPAU — Linux (NVIDIA). */
#if __has_include(<vdpau/vdpau.h>)
#if __has_include(<libavcodec/vdpau.h>)
#include <libavcodec/vdpau.h>
#endif
#if __has_include(<libavutil/hwcontext_vdpau.h>)
#include <libavutil/hwcontext_vdpau.h>
#endif
#endif

/* NVIDIA CUDA. */
#if (__has_include(<cuda.h>) || __has_include(<ffnvcodec/dynlink_cuda.h>)) && __has_include(<libavutil/hwcontext_cuda.h>)
#include <libavutil/hwcontext_cuda.h>
#endif

/* Intel Quick Sync (libmfx / libvpl). */
#if __has_include(<mfxvideo.h>)
#if __has_include(<libavcodec/qsv.h>)
#include <libavcodec/qsv.h>
#endif
#if __has_include(<libavutil/hwcontext_qsv.h>)
#include <libavutil/hwcontext_qsv.h>
#endif
#endif

/* OpenCL. */
#if (__has_include(<CL/cl.h>) || __has_include(<OpenCL/cl.h>)) && __has_include(<libavutil/hwcontext_opencl.h>)
#include <libavutil/hwcontext_opencl.h>
#endif

/* AMD AMF — Windows / Linux. */
#if __has_include(<AMF/core/Factory.h>) && __has_include(<libavutil/hwcontext_amf.h>)
#include <libavutil/hwcontext_amf.h>
#endif
