// WalPad 8G: this kernel has no unprivileged user namespaces, so Firefox's content sandbox cannot run
// (the session sets MOZ_DISABLE_CONTENT_SANDBOX=1).  Firefox then shows "Some of Firefox's security
// features may offer less protection on your current operating system" on every start; the limit is
// known and cannot be fixed in userland, so the bar is turned off.  (Updates and safe browsing stay on.)
pref("security.sandbox.warn_unprivileged_namespaces", false);
