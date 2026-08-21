#!/system/bin/sh

ui_print "***************************************"
ui_print "  USB SampleRate Changer WebUI v0.2.0"
ui_print "***************************************"
ui_print "- SolidJS WebUI + Rust controller"
ui_print "- No boot service; usable immediately"
ui_print "- KernelSU/APatch WebUI required"

touch "$MODPATH/skip_mount"
set_perm "$MODPATH/usbsrctl" 0 0 0755
set_perm "$MODPATH/USB_SampleRate_Changer.sh" 0 0 0755
set_perm "$MODPATH/functions3.shlib" 0 0 0644
set_perm_recursive "$MODPATH/templates" 0 0 0755 0644
set_perm_recursive "$MODPATH/extras" 0 0 0755 0644
set_perm_recursive "$MODPATH/webroot" 0 0 0755 0644
