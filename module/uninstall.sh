#!/system/bin/sh

MODDIR="${0%/*}"
STATE_ROOT="/data/adb/usb_samplerate_changer_webui"

if [ -x "$MODDIR/usbsrctl" ]; then
    "$MODDIR/usbsrctl" reset >/dev/null 2>&1 || :
fi

rm -rf "$STATE_ROOT"
