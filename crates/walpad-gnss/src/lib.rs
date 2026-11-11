//! walpad-gnss: a stand-in for Android's GNSS HAL.  MediaTek's `mnld` publishes positions only on
//! private datagram sockets for the HAL; this crate speaks that protocol (`wire`), reads the NMEA
//! it forwards (`nmea`), folds everything into a receiver state (`state`) and the daemon
//! (`daemon`) publishes it as /run/walpad/gnss.json plus an NMEA pty at /run/walpad/nmea.
pub mod daemon;
pub mod io;
pub mod nmea;
pub mod state;
pub mod wire;
