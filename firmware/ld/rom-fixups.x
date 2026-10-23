/* ESP32 ROM symbols the BT blobs in esp-wifi-sys-esp32 0.3.0 need but esp-rom-sys 0.1.5 lacks.
   Upstream fix: esp-hal main, esp-rom-sys/ld/esp32/rom/esp32.rom.ld. Delete this file once a
   released esp-rom-sys carries them (the link then fails with duplicate PROVIDEs: harmless, but noisy). */
PROVIDE ( ld_pscan_evt_start_cbk = 0x4003e924 );
PROVIDE ( ld_iscan_evt_start_cbk = 0x4003b58c );
PROVIDE ( ld_page_evt_start_cbk = 0x4003cf40 );
