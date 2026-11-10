// WalPad 8G: Firefox plays through the PulseAudio system daemon (walpad-sound; unix:/run/pulse/native).
// A sound starts after a tap: autoplay with sound stays blocked until the user interacts with the page
// (the policy "block-audio" and media.autoplay.default 1 are Firefox's own default).  No content
// sandbox in this kernel, so cubeb needs no audio IPC remoting.
pref("media.cubeb.backend", "pulse");
pref("media.cubeb.sandbox", false);
pref("media.autoplay.default", 1);
