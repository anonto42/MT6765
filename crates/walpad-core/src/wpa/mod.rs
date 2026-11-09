//! Wi-Fi control through the wpa_supplicant control socket (M4). walpad-wifi (the shell script that
//! brings up the MediaTek stack and runs DHCP) is NOT touched; this only talks to the supplicant.
pub mod ctrl;
pub mod fake;
pub mod manager;
pub mod parse;
pub mod psk;
