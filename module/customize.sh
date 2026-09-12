#!/system/bin/sh

ui_print "***************************************"
MODULE_VERSION="$(sed -n 's/^version=//p' "${MODPATH:-.}/module.prop" 2>/dev/null || true)"
ui_print "  USB SampleRate Changer WebUI v${MODULE_VERSION:-unknown}"
ui_print "***************************************"
ui_print "- SolidJS WebUI + Rust controller"
ui_print "- Usable immediately; optional boot reapply"
ui_print "- KernelSU/APatch WebUI required"

# Use the controller's read-only detector so install and runtime share the
# same definition of legacy XML support. Always regenerate on install/upgrade.
# AIDL and unavailable audio services are accepted in limited mode.
ui_print "- Detecting audio capabilities"
set_perm "$MODPATH/usbsrctl" 0 0 0755
CAPABILITY_TEMP="$MODPATH/.device-capabilities.conf.tmp"
CAPABILITY_FILE="$MODPATH/device-capabilities.conf"
if ! "$MODPATH/usbsrctl" _probe-device > "$CAPABILITY_TEMP"; then
    ui_print "! Device probe unavailable; installing in limited mode"
    printf 'version=1\naudio_hal=unknown\npolicy_xml_supported=0\nreason=install_probe_unavailable\n' > "$CAPABILITY_TEMP" \
        || abort "! Cannot write device capabilities"
fi
set_perm "$CAPABILITY_TEMP" 0 0 0644
mv -f "$CAPABILITY_TEMP" "$CAPABILITY_FILE" || abort "! Cannot save device capabilities"
if grep -q '^policy_xml_supported=1$' "$CAPABILITY_FILE"; then
    ui_print "- Traditional XML controls available (installation result saved)"
else
    ui_print "- Limited mode: resampler, diagnostics and all jitter features"
    ui_print "- Policy, Bluetooth HAL switching and USB period are hidden"
fi

# One-time install/upgrade migration for the legacy controller layout. Keep
# every removal target as a literal absolute path so runtime variables cannot
# broaden or redirect the deletion scope.
ui_print "- Migrating legacy controller runtime files"
if [ -L "/data/adb/usb_samplerate_changer_webui/generated" ]; then
    rm -f "/data/adb/usb_samplerate_changer_webui/generated"
elif [ -d "/data/adb/usb_samplerate_changer_webui/generated" ]; then
    rm -rf "/data/adb/usb_samplerate_changer_webui/generated"
fi
if [ -f "/data/adb/usb_samplerate_changer_webui/last.log" ] || [ -L "/data/adb/usb_samplerate_changer_webui/last.log" ]; then
    rm -f "/data/adb/usb_samplerate_changer_webui/last.log"
fi
if [ -f "/data/adb/usb_samplerate_changer_webui/last.status" ] || [ -L "/data/adb/usb_samplerate_changer_webui/last.status" ]; then
    rm -f "/data/adb/usb_samplerate_changer_webui/last.status"
fi
if [ -f "/data/adb/usb_samplerate_changer_webui/last-command.log" ] || [ -L "/data/adb/usb_samplerate_changer_webui/last-command.log" ]; then
    rm -f "/data/adb/usb_samplerate_changer_webui/last-command.log"
fi
if [ -f "/data/adb/usb_samplerate_changer_webui/boot-reapply.log" ] || [ -L "/data/adb/usb_samplerate_changer_webui/boot-reapply.log" ]; then
    rm -f "/data/adb/usb_samplerate_changer_webui/boot-reapply.log"
fi

touch "$MODPATH/skip_mount"
set_perm "$MODPATH/usbsrctl" 0 0 0755
set_perm "$MODPATH/core/USB_SampleRate_Changer.sh" 0 0 0755
set_perm "$MODPATH/core/functions3.shlib" 0 0 0644
set_perm_recursive "$MODPATH/core/templates" 0 0 0755 0644
set_perm_recursive "$MODPATH/core/extras" 0 0 0755 0644
set_perm_recursive "$MODPATH/webroot" 0 0 0755 0644
