#!/system/bin/sh

MODDIR="${0%/*}"
STATE_ROOT="/data/adb/usb_samplerate_changer_webui"
LOG_ROOT="/data/local/tmp/usb_samplerate_changer_webui"
reset_failed=0

if [ -x "$MODDIR/usbsrctl" ]; then
    # Cleanup ignores settings state health and filters resets by device capabilities.
    # Full mode restores Bluetooth HAL defaults; limited mode skips HAL/USB resets.
    # Audio services restart once before the module files disappear.
    "$MODDIR/usbsrctl" cleanup >/dev/null 2>&1 || reset_failed=1
else
    reset_failed=1
fi

rm -rf "/data/adb/usb_samplerate_changer_webui"
rm -rf "/data/local/tmp/usb_samplerate_changer_webui"

if [ "$reset_failed" -ne 0 ]; then
    echo "USB SampleRate Changer: some audio settings could not be reset during uninstall" 1>&2
fi
