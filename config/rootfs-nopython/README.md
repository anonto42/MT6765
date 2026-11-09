# config/rootfs-nopython: overlay for the Python-free image (card M7.1)

`WALPAD_NO_PYTHON=1 scripts/build_rootfs.sh` builds the image from `config/rootfs`, then
`apply.sh` removes the paths listed in `REMOVE` and copies `files/` over the result.
Default builds never read this directory.  Details: `docs/rust/reports/M7.1.md`.
