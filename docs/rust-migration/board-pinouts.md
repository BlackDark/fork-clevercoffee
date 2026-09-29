# Board pinouts

Pin maps for the three target boards, from Espressif's own documentation. Every row is tagged
with the source revision it was read from. This replaces the "S3 and C6 pin maps do not exist"
open question from the Part A report: they exist here, and one of them does not fit.

Evidence date: 2026-09-29.
Sources: `espressif/esp-dev-kits` @ `ceaefbd43f80c01496818612b8b874b04eca8038`, `espressif/esp-idf`
@ `4d59230ddff16327812782151ef0afef202dc6d7`, ESP32 Datasheet v5.3, ESP32-S3 Datasheet v2.2,
ESP32-C6 Datasheet v1.5.

Cross-links: [compatibility-matrix.md](compatibility-matrix.md),
[task-list.md](task-list.md), [architecture.md](architecture.md#17-board-variation).

---

## 1. The board each target uses

| Target | Board | Module | Flash | PSRAM |
| --- | --- | --- | --- | --- |
| `esp32` | ESP32-DevKitC V4 | ESP32-WROOM-32E (the variant shown in the vendor photo) | module-dependent, **4 MB on WROOM-32E** | none |
| `esp32s3` | ESP32-S3-DevKitC-1 v1.1 | ESP32-S3-WROOM-1-N8R8 | 8 MB quad | 8 MB octal |
| `esp32c6` | ESP32-C6-DevKitC-1 v1.2 | ESP32-C6-WROOM-1(U) | 8 MB | none |

`needs confirmation` on the regulator and USB-bridge part numbers for all three: the vendor user
guides say only "5 V to 3.3 V LDO" and "single USB-to-UART bridge chip ... up to 3 Mbps" without
naming the part, and the schematics were not read.

## 2. How many pins this project actually needs

Counted from the signal list in [inventory.md](inventory.md#21-pin-map-includeclevercoffeehardwarepinmappingh):

| Signal | Pins |
| --- | --- |
| Relays: heater, pump, valve | 3 output |
| LEDs: status, brew, steam | 3 output |
| Panel switches: power, brew, steam, hot water | 4 input |
| Water tank switch | 1 input |
| Temperature sensor, bit-banged 1-Wire | 1 bidirectional |
| HX711 scale, 2 data + 1 clock | 3 (optional) |
| I2C, SDA + SCL, shared with the pressure sensor | 2 |
| **Total** | **17** |

The C++ firmware additionally declares a zero-cross pin and three rotary-encoder pins that no code
references, so they are not counted.

## 3. The finding that changes the plan

**The ESP32-C6-DevKitC-1 does not have enough spare pins for the full feature set.**

Counting from the vendor's own J1 and J3 tables:

| Step | Count |
| --- | --- |
| GPIOs exposed on the header | **23**: 0-13, 15-23 |
| less USB D- and D+ (GPIO12, 13) | 21 |
| less the module's SDIO flash bus (GPIO18, 19, 20, 21, 22, 23) | 15 |
| less the on-board RGB LED (GPIO8) | **14 usable** |

The C6 vendor guide contradicts itself about the flash pins: it states "All available GPIO pins
(except for the SPI bus for flash) are broken out to the pin headers", yet its own J3 table lists
all six SDIO pins as exposed. The module carries its flash, so those six are treated as unusable
pending a schematic check. That assumption is the conservative one, and the conclusion holds
either way: 17 needed, 14 usable.

The ESP32 and the ESP32-S3 both expose comfortably more than 17.

### What fits on the C6, and what does not

The full signal set is 17 pins. Broken into essential and optional:

| Group | Pins | Essential? |
| --- | --- | --- |
| Relays: heater, pump, valve | 3 | yes |
| Panel switches: power, brew, steam, hot water | 4 | yes |
| Water tank input | 1 | yes |
| 1-Wire temperature | 1 | yes |
| I2C SDA + SCL | 2 | yes |
| **Essential subtotal** | **11** | |
| HX711 single-cell: data + clock | 2 | needed for brew-by-weight |
| HX711 second cell data | 1 | dual-cell only |
| LEDs: status, brew, steam | 3 | cosmetic |

**14 usable covers the essential 11 plus a single-cell HX711 (2), with one pin spare.** The two
things that do not fit are the three indicator LEDs and the second HX711 load cell.

### C6 map

Resolved by the user on 2026-09-29: **features are disabled on the C6** rather than adding an
expander or a different board.

| Signal | GPIO | Note |
| --- | --- | --- |
| Heater relay | 10 | plain GPIO |
| Pump relay | 11 | plain GPIO |
| Valve relay | 2 | LP_UART_RTSN, ADC1_CH2, FSPICQ; not a strapping pin |
| Power switch | 0 | XTAL_32K_P; the 32 kHz crystal is not used, so this is free |
| Brew switch | 1 | XTAL_32K_N; same |
| Steam switch | 3 | LP_UART_CTSN, ADC1_CH3 |
| Hot water switch | 15 | strapping, but a switch is passive and the pin is only sampled at reset |
| Water tank | 4 | MTMS strapping, floating at reset; fine for an input |
| 1-Wire temperature | 5 | MTDI strapping; 1-Wire idles high, so the sample is correct |
| HX711 data / clock (single cell) | 6 / 7 | MTCK / MTDO, and the C6's default LP_I2C pins; usable as GPIO |
| I2C SDA / SCL | 9 / 16 | 9 is a strapping pin that idles high, correct for an open-drain bus. 16 is UART0 TX: given up because C6 provisioning uses native USB, not the bridge |
| Spare | 17 | UART0 RX, available if a feature needs it |

Every assignment avoids the flash bus, USB, the RGB LED and the strapping pins whose sampled
level would be wrong.

**Disabled on the C6:** the three indicator LEDs, and the second HX711 load cell, so the C6
supports a single-cell scale only. Everything else is identical to the ESP32 and the S3, and the
shared logic does not change: the LED and second-cell code paths are simply not compiled for the
C6 board feature.

The alternative, adding an IO expander, was rejected by the user. It would also have raised an
unanswered question about heater PWM at a 10 ms window over I2C.

## 4. Per-chip pin facts

### 4.1 Input-only pins

| Chip | Input-only GPIO | Internal pull available |
| --- | --- | --- |
| ESP32 | **34, 35, 36, 37, 38, 39** | **no** |
| ESP32-S3 | none | yes, on every pin |
| ESP32-C6 | none | yes, on every pin |

ESP32 Datasheet v5.3, section 2: "Input only pins, output is not supported due to lack of
pull-up/pull-down resistors." ESP-IDF `components/soc/esp32/include/soc/soc_caps.h:183-184`:
`// GPIO >= 34 are input only`.

ESP-IDF `soc_caps.h` for S3 and C6: `// No GPIO is input only`.

The C++ firmware puts all four panel switches on input-only pins 34, 35, 36, 39 with
`pinMode(pin, INPUT)` and no internal pull
(`src/hardware/GPIOPin.cpp:47-50`), so the board **must** supply external pull-ups or pull-downs.
That is a hardware requirement, not a firmware choice, and it does not carry to the S3 or C6,
where an internal pull is available and preferable.

### 4.2 ADC2 and Wi-Fi

| Chip | ADC2 | Conflict |
| --- | --- | --- |
| ESP32 | ADC2_CH0 to CH9, on GPIO0, 2, 4, 12, 13, 14, 15, 25, 26, 27 | yes, ADC2 is used by Wi-Fi |
| ESP32-S3 | ADC2_CH0 to CH9, on GPIO11 to 20 | yes |
| ESP32-C6 | **no ADC2**, ADC1_CH0 to CH6 on GPIO0 to 6 only | no |

ESP-IDF `docs/en/api-reference/peripherals/adc/adc_oneshot.rst:165`: "ADC2 is also used by Wi-Fi."

Consequence: this project uses **no analogue inputs at all**, so the conflict is moot on all three
chips. It is recorded because it constrains any future analogue sensor, and because the C++ pin
map happens to sit on ADC2 pins for the relays (GPIO25, 26, 27) where it does not matter.

### 4.3 Strapping pins

| Chip | Strapping pins | Why it matters |
| --- | --- | --- |
| ESP32 | **GPIO0** boot mode, **GPIO2** boot mode, **GPIO5** SDIO timing, **GPIO12** VDD_SDIO voltage, **GPIO15** U0TXD printing and SDIO timing | the C++ pin map puts the heater relay on **GPIO2**, a strapping pin, and the LEDs on **GPIO1**, UART0 TX |
| ESP32-S3 | **GPIO0** boot mode, **GPIO3**, **GPIO45** VDD_SPI voltage, **GPIO46** boot mode | none of these are in the C++ pin map, but GPIO45 and 46 are also the two pins to avoid |
| ESP32-C6 | **GPIO4** MTMS, **GPIO5** MTDI, **GPIO8**, **GPIO9** (weak pull-up), **GPIO15** JTAG source | **GPIO9 is a strapping pin with a weak pull-up**, and 8 and 9 are the S3-style I2C pair people reach for |

The ESP32 datasheet is explicit that being a strapping pin does not forbid use, it forbids
mis-driving at reset: "the pins are freed up to be used as regular IO pins after reset." A relay
on a strapping pin is therefore workable but must be in its inactive state when the pin is
sampled, which the C++ firmware does not guarantee because relays are created late in startup
(`src/hardware/HardwareManager.cpp:70-93`). The new design drives the actuators off before
anything else, so the strapping state is correct at reset.

### 4.4 Reserved and dangerous pins

| Chip | Not usable | Reason |
| --- | --- | --- |
| ESP32 | **GPIO6, 7, 8, 9, 10, 11** (D0, D1, D2, D3, CMD, CLK) | vendor states: "used internally for communication between ESP32 and SPI flash memory ... Avoid using these pins" |
| ESP32 | **GPIO1, GPIO3** | UART0 TX/RX, wired to the USB-UART bridge |
| ESP32 | GPIO16, GPIO17 | reserved on WROVER module variants; usable on WROOM and SOLO-1 |
| ESP32-S3 | **GPIO19, GPIO20** | native USB D- and D+ |
| ESP32-S3 | GPIO43, GPIO44 | UART0 TX/RX |
| ESP32-S3 | GPIO39, 40, 41, 42 | JTAG |
| ESP32-S3 | **GPIO35, 36, 37** on octal-flash and WROOM-2 variants | "used for the internal communication between ESP32-S3 and SPI flash/PSRAM memory" |
| ESP32-S3 | GPIO38 or GPIO48 on v1.1 or v1.0 | the on-board RGB LED moved between revisions |
| ESP32-C6 | **GPIO12, GPIO13** | native USB D- and D+ |
| ESP32-C6 | GPIO16, GPIO17 | UART0 TX/RX |
| ESP32-C6 | GPIO18 to GPIO23 | the module's SDIO flash bus, exposed on J3 |
| ESP32-C6 | GPIO8 | the on-board RGB LED |

The S3 revision trap is worth stating plainly: ESP32-S3-DevKitC-1 **v1.0 and v1.1 differ**, and
the RGB LED is on GPIO48 on v1.0 and GPIO38 on v1.1. A pin map written against one can hit the
RGB LED on the other.

## 5. Proposed pin maps

Same signal order on all three boards so the shared logic is identical; only the numbers differ.
Every assignment avoids strapping pins where an alternative exists, avoids flash, USB, UART0 and
JTAG pins, and prefers ADC1 for anything that might later become analogue.

### 5.1 ESP32, ESP32-DevKitC V4

Kept as close to the C++ map as possible, because an existing machine has wiring soldered to it.
The three changes are the ones the evidence forces.

| Signal | C++ | Rust | Why it changed |
| --- | --- | --- | --- |
| Heater relay | 2 | **4** | GPIO2 is a boot-mode strapping pin; 4 is ADC2 (unused by this project) |
| Pump relay | 27 | 27 | keep, avoids the flash pins |
| Valve relay | 17 | 17 | keep |
| Status LED | 26 | 26 | keep |
| Brew LED | 19 | 19 | keep |
| Steam LED | 1 | **21** | GPIO1 is UART0 TX; 21 is free once I2C moves to 21/15 below |
| Power switch | 39 | 39 | keep, input-only, external pull required |
| Brew switch | 34 | 34 | keep |
| Steam switch | 35 | 35 | keep |
| Hot water switch | 36 | 36 | keep |
| Water tank | 23 | 23 | keep, internal pull selectable |
| 1-Wire temp | 16 | 16 | keep |
| HX711 D1 / D2 / CLK | 32 / 25 / 33 | 32 / 25 / 33 | keep |
| I2C SDA / SCL | 21 / 22 | **22 / 15** | GPIO21 was the steam LED, which moved to 23; but 23 is the water tank, so SDA takes 21 back and SCL moves to 15. GPIO15 is a strapping pin sampled only at reset, and I2C idles high, so the sample is correct. |

Final ESP32 map: heater 4, pump 27, valve 17, status LED 26, brew LED 19, steam LED 21, power
switch 39, brew switch 34, steam switch 35, hot water switch 36, water tank 23, 1-Wire 16, HX711
32/25/33, I2C SDA 21 / SCL 15. No pin collides with the flash bus, USB, UART0 or an
unavailable input.

### 5.2 ESP32-S3, ESP32-S3-DevKitC-1 v1.1

| Signal | GPIO | Note |
| --- | --- | --- |
| Heater relay | 4 | ADC1_CH3, header J1-4 |
| Pump relay | 5 | |
| Valve relay | 6 | |
| Status LED | 21 | |
| Brew LED | 47 | |
| Steam LED | 48 | |
| Power switch | 1 | internal pull available, no external resistor needed |
| Brew switch | 2 | |
| Steam switch | 3 | strapping, but only sampled at reset and a switch is passive |
| Hot water switch | 10 | |
| Water tank | 11 | |
| 1-Wire temp | 12 | |
| HX711 D1 / D2 / CLK | 13 / 14 / 16 | |
| I2C SDA / SCL | **8 / 9** | the S3 default, both ADC1, both on the header |

### 5.3 ESP32-C6, ESP32-C6-DevKitC-1 v1.2

Per [the C6 map above](#c6-map). Three of the seventeen signals do not fit, so the map assigns
the other fourteen and the LEDs and the second load cell are disabled for this board.

## 6. USB and the provisioning transport, per board

| Board | Ports | Native USB | Provisioning transport |
| --- | --- | --- | --- |
| ESP32-DevKitC V4 | one Micro-USB to a USB-UART bridge | **no** | UART0 through the bridge |
| ESP32-S3-DevKitC-1 v1.1 | two Micro-USB: a bridge, and the native USB OTG | yes, USB Serial/JTAG | either; the native port is the default |
| ESP32-C6-DevKitC-1 v1.2 | two USB-C: a bridge, and the native USB | yes, USB Serial/JTAG | either |

This confirms the [decision record](decision-record.md#decision-usb-provisioning): UART0 on the
original ESP32, USB Serial/JTAG on S3 and C6. Both S3 and C6 expose a UART0 bridge as well, so
the firmware can support both transports on those boards and the user picks by plugging into the
right port.

Note the native USB is **full speed, 12 Mbps**, not high speed. A 100 KB firmware image transfers
in well under a second, so this does not matter for flashing, but it does bound the config import
chunk rate.

## 7. Power notes for a relay board

| Board | Input | Regulator | Note |
| --- | --- | --- | --- |
| ESP32-DevKitC V4 | 5 V or 3V3, one and only one | 5 V to 3.3 V LDO, part number `needs confirmation` | the ESP32 datasheet asks for 500 mA or more from a 3.3 V supply; three relay coils and a heater SSR will exceed that, so they need a separate supply and a common ground |
| ESP32-S3-DevKitC-1 v1.1 | 5 V or 3V3 | 5 V to 3.3 V LDO, part `needs confirmation` | the N32R16V variant runs its SPI rail at 1.8 V, so it is not interchangeable with the N8R8 on a mixed-voltage board |
| ESP32-C6-DevKitC-1 v1.2 | 5 V or 3V3 | 5 V to 3.3 V LDO, part `needs confirmation` | J5 jumper must be removed when powering from the 3V3 header to measure current |

The relay coils must not be powered from the dev board's 3.3 V rail. That is a hardware
instruction, and it is outside what this migration can verify.

## 8. Open questions this document does not resolve

| Question | What would resolve it |
| --- | --- |
| Which C6 board or feature reduction | the user |
| Regulator and USB-bridge part numbers | the three schematics, linked from the vendor user guides |
| Whether the C6's J3 really routes the six SDIO flash pins | the C6 v1.2 schematic |
| ESP32-DevKitC flash size on the actual board | the WROOM-32E module marking |
| S3 board revision, v1.0 or v1.1 | the board itself; it changes the RGB LED pin |
| The ESP32 GPIO46 direction question, input-only or not | the S3 datasheet does not say; ESP-IDF says no S3 pin is input-only |
