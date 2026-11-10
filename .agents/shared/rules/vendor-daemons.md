# Android Vendor Daemons on Alpine

- Run through Android's bootstrap linker from `/system/system/bin/bootstrap/` (`linker64` for
  64-bit, `linker` for 32-bit; check `head -c5 bin | od` byte 5: 01 = 32-bit).
- Libraries: 64-bit `LD_LIBRARY_PATH=$SYS/lib64/bootstrap:/vendor/lib64:$SYS/lib64`; 32-bit
  `$SYS/lib/bootstrap:/vendor/lib:$SYS/lib` plus the VNDK APEX `lib` (mounted by walpad-gps at
  `/run/walpad/apex/vndk`) when a library is missing.
- Properties: `LD_PRELOAD=/usr/local/lib/libwpdprops.so` (64) or `libwpdprops32.so` (32); shared
  file `/tmp/wpd-props`; load `build.prop`s and `init.svc.<name>=running` as Android init would.
- Process name: start through a symlink `/run/walpad/android-bin/<name>` -> linker (kernel
  drivers check `comm`).
- Logs: `socat -u UNIX-RECV:/dev/socket/logdw OPEN:/var/log/walpad-android.log,creat,append`;
  read with `strings`.
- Find Android init args in `/vendor/etc/init/*.rc`; debug with `strace -f -e trace=openat,connect`.
- Kernel modules from `/vendor/lib/modules` follow `modules.dep`; never rmmod CONNSYS modules.
- Scripts: `walpad-wifi` (CONNSYS: do not edit stack_up), `walpad-modem`, `walpad-gps`.
