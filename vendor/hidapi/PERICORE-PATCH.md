# Vendored hidapi 2.6.7 (MIT, see LICENSE.txt)

Unmodified copy of the crates.io release except one change in `src/lib.rs`,
`HidApi::new()`: when `HidApi::disable_device_discovery()` was called, the
initial `add_devices(0, 0)` is skipped. Upstream skips it only for libusb, so
on Windows `new()` read the strings of every HID device, and a built-in
Synaptics control (06CB:1595) takes 10 s per interface to answer, stalling
PeriCore's start-up and battery reads by 20 s. PeriCore lists its own devices
(`src/devices/hidlist.rs`) and only uses hidapi to open them by path.

The bundled C sources (`etc/`) are left out; the `windows-native` backend
does not use them.
