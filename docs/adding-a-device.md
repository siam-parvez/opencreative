# Adding a device

OpenCreative supports the DeepCool LM240 and LM360 today. Other DeepCool
screens (Mystique, LQ, LP, the AK/LS/LD digital series, CH cases) each speak
their own USB protocol, and we can only add one we can see. If you own one,
here's what helps, from most to least important.

## 1. Who you are and what you have (5 minutes, Linux)

The easiest way: in OpenCreative, open **Devices → Request support for your
cooler**. It opens the form on GitHub with your cooler's USB details already
filled in; add the model name and you're done.

Or open a [new device issue](https://github.com/siam-parvez/opencreative/issues/new?template=new-device.yml)
yourself with:

- **The exact model** from the box (e.g. "LQ360 White").
- **`lsusb`**: the line with `DeepCool` or ID `3633:xxxx`.
- **`lsusb -v -d 3633:`** (run with `sudo`): interfaces and endpoints tell
  us whether the device is plain USB bulk (like the LM series) or HID.
- **`opencreative list`** output, and what happens with
  `opencreative --no-daemon solid ff0000` (a red screen, nothing, or an
  error).
- **Photos**: the device front, and the screen showing something in
  DeepCreative if you have it.

## 2. A USB capture of DeepCreative (Windows, 20 minutes)

This is what lets us write the protocol. DeepCreative only runs on Windows:

1. Install [Wireshark](https://www.wireshark.org/) and tick **USBPcap**
   during setup. Reboot.
2. Open Wireshark, pick the **USBPcap** interface the cooler is on, start
   capturing.
3. In DeepCreative, do one thing at a time, waiting a few seconds between
   each, and **write down the time of each step**:
   - open DeepCreative (the handshake)
   - change brightness to 0, then to 100
   - turn the screen off and on (Zen mode)
   - show a solid-color image
   - switch to a digital/monitor layout
   - rotate the screen
4. Stop, then **File → Export Specified Packets**, filtered with
   `usb.idVendor == 0x3633 || usb.addr contains "x.y"` (your device's
   address), saved as `.pcapng`.
5. Attach the file to the issue with your list of steps and times.

Captures contain only what DeepCreative sends to the cooler; no personal
data. Don't capture other USB devices (keyboards!) at the same time.

## 3. Test builds

Once there's a first try at your device, we'll ask you to run a test build
and tell us what the screen shows. Photos of the screen are the fastest
feedback.

## What we do with it

The protocol goes into `docs/protocol/<series>.md` with the bytes that
prove it, then a `Display` implementation in `crates/opencreative-device`
and an entry in `SUPPORTED`. See [lm-series.md](protocol/lm-series.md)
for what a finished protocol write-up looks like.
