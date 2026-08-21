# USB SampleRate Changer WebUI

This repository packages [SivonZn/USB_SampleRate_Changer](https://github.com/SivonZn/USB_SampleRate_Changer), a clean Fork of the [original project](https://github.com/yzyhk904/USB_SampleRate_Changer), as a KernelSU/APatch module WebUI. It has no boot service: opening the module WebUI calls the bundled `usbsrctl` controller on demand.

The Fork source is attached as the [`USB_SampleRate_Changer`](USB_SampleRate_Changer) Git Submodule. The parent repository's Gitlink pins an exact Fork commit, and the patches listed in [`patches/series`](patches/series) are applied only to a temporary copy while assembling the module. The Submodule remains clean and can be used directly to investigate issues, develop general-purpose fixes, and propose them to the original repository independently.

## Repository layout

| Path | Purpose |
| --- | --- |
| `USB_SampleRate_Changer/` | Fork Submodule and reproducible source commit |
| `patches/` | Temporary downstream changes awaiting upstream validation |
| `module/` | KernelSU/APatch module metadata and installer overlay |
| `backend/` | Rust command validator and controller |
| `webui/` | SolidJS WebUI source |
| `build.sh` | Fetch, patch, compile and package pipeline |

## Parameter mapping

The Rust controller exposes every behavior-changing parameter parsed by `USB_SampleRate_Changer.sh`:

| UI control | Upstream parameter |
| --- | --- |
| Policy selector | `--auto`, `--offload`, `--offload-hifi-playback`, `--offload-direct`, `--offload-safer`, `--bypass-offload`, `--bypass-offload-safer`, `--legacy`, `--safe`, `--safest`, `--safest-auto`, `--usb-only` |
| Sample-rate selector | documented 44.1–768 kHz presets, plus a custom integer in the upstream-supported 44100–768000 Hz range |
| Bit-depth selector | `16`, `24`, `32`, `float` |
| DRC switch | `--drc` |
| Force USBv2 switch | `--force-usbv2` |
| Force Bluetooth QTI switch | `--force-bluetooth-qti` |
| Bluetooth A2DP route check | Automatically runs after an audio change only when A2DP was connected before the operation |
| Amazon Music template switch | `--amzm` |
| Test-template switch and selector | `--test --test-template NAME` |
| Reset button | `--reset` |

## Extras mapping

The upstream `extras/` directory is also exposed through fixed Rust subcommands. The WebUI never accepts an arbitrary script name or free-form shell command.

| UI section | Upstream script and supported controls |
| --- | --- |
| Bluetooth HAL | `change-bluetooth-hal.sh`: status, `aosp`, `legacy`, `offload`, `sysbta` |
| AudioFlinger resampler | `change-resampling-quality.sh`: status/reset, every bundled preset, or validated custom stop-band (20–242 dB), half-filter length (8–640 in steps of 8), cutoff/cheat percentage, and 44.1/48/96 kHz activation threshold |
| USB transfer period | `change-usb-period.sh`: status/reset and the complete 125–50000 µs range in steps of 125 µs |
| Jitter reducer | `jitter-reducer.sh`: SELinux, thermal, Doze, governor, camera, logd, I/O, VM, Wi-Fi, battery and effects; I/O scheduler/tone and Wi-Fi no-restart are supported |
| Diagnostics | filtered/full audio dumps, Bluetooth dumps, detected configuration and ALSA hardware parameters |

SELinux and thermal reducer activation require a second confirmation in the WebUI. They can weaken device security or thermal protection. Jitter switches represent pending actions rather than durable state: most upstream jitter changes are not persistent, while its Wi-Fi behavior may persist across reboot.

`--help` is informational and is not persisted. Policy flags are mutually exclusive. The WebUI also makes `--amzm` and `--test` mutually exclusive because both override the selected policy template and the latter wins only by argument-processing order.

The upstream default `templates/test_template.xml` does not exist, so test mode always requires selecting one of the XML files enumerated from the bundled `templates/` directory. Arbitrary paths are rejected.

## Execution path

```text
SolidJS WebUI
  -> window.ksu.exec(usbsrctl apply ... | usbsrctl extra ...)
  -> Rust validates a fixed option schema
  -> /data/adb/usb_samplerate_changer_webui/generated/*.sh
  -> select the audioserver mount namespace when required
  -> execute the selected bundled upstream script
```

When the controller starts outside the audioserver namespace, it retries the generated script through `su --mount-master`. Audio-policy apply/reset scripts perform the namespace comparison again, so a silently failed namespace switch cannot produce a false success. Extra scripts use the same controller routing and operation lock.

Restarting `audioserver` can leave an already connected Bluetooth headset registered only as SCO, even though the Bluetooth stack still has an active A2DP codec. The controller records whether A2DP was connected before apply/reset and verifies both the connected-device list and the `STREAM_MUSIC` route after the reload. The WebUI warns before applying while A2DP is connected. If the post-apply route check fails, the controller returns code 72 and the WebUI asks whether to open Bluetooth settings for an explicit disconnect/reconnect instead of navigating there automatically or reporting a false success.

The latest configuration, generated script, status and output are stored under `/data/adb/usb_samplerate_changer_webui/` with root-only permissions.

## Build

Node.js, Rust, the `aarch64-linux-android` Rust target, Android NDK and `zip` must be installed. Clone this repository with its Submodule, or initialize it after cloning:

```sh
git clone --recurse-submodules https://github.com/SivonZn/USB_SampleRate_Changer_WebUI.git
# Existing clone:
git submodule update --init --recursive
```

The build copies the current Submodule worktree into a temporary directory, verifies and applies every listed Patch there, builds both applications, and creates the final module under the parent Magisk workspace's `output/` directory:

```sh
./build.sh
```

Uncommitted changes inside the Submodule are intentionally included in local builds. This makes it possible to modify the upstream scripts and immediately test the assembled module without committing first:

```sh
cd USB_SampleRate_Changer
git switch -c fix/example
# Edit or commit the upstream fix, then return to the WebUI repository.
cd ..
./build.sh
```

Before recording a new Submodule revision in this repository, commit and push the corresponding change to the Fork, then stage the updated Gitlink with `git add USB_SampleRate_Changer`.

The current artifact name is `USB_SampleRate_Changer_WebUI-0.3.0.zip`. Rebuilding the same version replaces that exact file.

For development checks:

```sh
cargo test --manifest-path backend/Cargo.toml
cd webui && npm run build
```
