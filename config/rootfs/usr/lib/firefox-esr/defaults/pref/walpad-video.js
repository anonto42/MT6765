// WalPad 8G: no GPU video decoding and a software-rendered X screen, so YouTube must get the cheapest
// codec.  VP9/AV1 in software starve the CPU (RDD ~90 %, Firefox ~110 %, fbmirror ~90 % of 8 weak
// cores: choppy video); with both off YouTube serves H.264 (libavcodec is in the image), much lighter.
// (Re-enabled 2026-11-11: the "playback does not begin" error was the silent sound sink, not the codec.)
pref("media.mediasource.vp9.enabled", false);
pref("media.av1.enabled", false);
pref("media.ffvpx.enabled", true);
pref("media.hardware-video-decoding.enabled", false);
pref("media.ffmpeg.vaapi.enabled", false);
pref("widget.dmabuf.force-enabled", false);
pref("gfx.webrender.software", true);
pref("layers.acceleration.disabled", true);
pref("gfx.x11-egl.force-disabled", true);
pref("general.smoothScroll", false);
pref("dom.min_background_timeout_value", 4000);
// the software compositor (WebRender, SwComposite thread) was ~120 % of a core while a video played;
// YouTube is 30 fps, so drawing the page at 60 fps only burns CPU
pref("layout.frame_rate", 30);
