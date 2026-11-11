// WalPad 8G: no GPU video decoding and a 1280x1280 software-rendered X screen, so YouTube must get the
// cheapest codec.  VP9/AV1 in software starve the CPU (Firefox ~180 %, fbmirror ~80 %: stutter, audio
// underruns); with both off YouTube serves H.264 (ffmpeg libavcodec is in the image), several times lighter.
pref("media.mediasource.vp9.enabled", false);
pref("media.av1.enabled", false);
pref("media.ffvpx.enabled", true);
pref("media.hardware-video-decoding.enabled", false);
// fewer wasted frames: no smooth-scroll animation work and no background tab timers
pref("general.smoothScroll", false);
pref("dom.min_background_timeout_value", 4000);
