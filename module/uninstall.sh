#!/system/bin/sh

MODDIR="${0%/*}"
STATE_ROOT="/data/adb/usb_samplerate_changer_webui"
LOG_ROOT="/data/local/tmp/usb_samplerate_changer_webui"
reset_failed=0

run_reset() {
    if [ -x "$MODDIR/usbsrctl" ]; then
        "$MODDIR/usbsrctl" "$@" >/dev/null 2>&1 || reset_failed=1
    else
        reset_failed=1
    fi
}

if [ -x "$MODDIR/usbsrctl" ]; then
    # Query through the controller's validated state reader; never source state
    # as shell code. Restore only settings this module actually managed.
    saved_status="$("$MODDIR/usbsrctl" status 2>/dev/null)"
    if [ "$?" -ne 0 ] || printf '%s\n' "$saved_status" | grep -q '^state_degraded=1$'; then
        reset_failed=1
    else
        if printf '%s\n' "$saved_status" | grep -q '^resampler_configured=1$'; then
            run_reset extra resampler reset
        fi
        if printf '%s\n' "$saved_status" | grep -q '^usb_period_configured=1$'; then
            run_reset extra usb-period reset
        fi
        for feature in selinux thermal doze governor camera logd io vm wifi battery effect; do
            if printf '%s\n' "$saved_status" | grep -q "^jitter_${feature}_configured=1$"; then
                run_reset extra jitter disable "$feature"
            fi
        done
    fi
    # Artifact cleanup remains possible when legacy policy application is
    # unavailable. The controller avoids vendor HAL restart in limited mode.
    if printf '%s\n' "$saved_status" | grep -q '^policy_configured=1$' \
        || [ -e "$MODDIR/core/.config" ] \
        || [ -e /data/local/tmp/audio_conf_generated.xml ]; then
        run_reset reset
    fi
    # Bluetooth HAL properties are intentionally left untouched.

fi

rm -rf "/data/adb/usb_samplerate_changer_webui"
rm -rf "/data/local/tmp/usb_samplerate_changer_webui"

if [ "$reset_failed" -ne 0 ]; then
    echo "USB SampleRate Changer: some audio settings could not be reset during uninstall" 1>&2
fi
