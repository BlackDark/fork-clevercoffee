//! Firmware bring-up: pins, configuration, peripherals, and the tasks.
//!
//! Drive every actuator inactive and read each pin back before anything else runs.
//! The heater ISR starts disarmed; only the control task feeds the watchdog and
//! beats the deadman gate that permits heater duty.
//!
//! # Hardware limits
//!
//! Inactive is `Level::Low`, matching `HIGH_TRIGGER` relays. On `LOW_TRIGGER`
//! hardware that level energises outputs; safety validation rejects that heater
//! configuration. Confirm board polarity before energising the boiler.
//! The heater still requires the disconnected-boiler dummy-load procedure;
//! see [`cc_hal_esp32::heater`] for unmeasured contactor timing and pin duty.

use core::error::Error;
use std::sync::Arc;

use cc_hal_esp32::heater::{HeaterOutput, TimerIsrPwm};
use cc_hal_esp32::pins;
use cc_hal_esp32::SwitchBank;
use esp_idf_hal::gpio::{InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripherals::Peripherals;
use log::{error, info, warn};

use crate::probe::bring_up_temperature_sensor;
use crate::{control_task, display_task, network, slots, EspError};

/// The level that de-energises a `HIGH_TRIGGER` relay — what the bring-up
/// sequence drives each actuator pin to before anything reads it back.
///
/// **Why a constant and not the configuration:** a `LOW_TRIGGER` relay energises
/// whenever its pin *floats*, which is before this firmware runs, so driving
/// such a pin low to "de-energise" it would **energise** it.
/// `cc_safety::validate_config` refuses `LOW_TRIGGER` for every relay for
/// exactly that reason, which means a stored configuration carrying one is
/// discarded at boot and the defaults — high-trigger — are what run here. The
/// relay *drivers* do honour the configured polarity
/// (`actuators::Polarity`); this is the one place where honouring it would be
/// the unsafe choice, and it is the reason the two differ.
const INACTIVE: Level = Level::Low;

// The original ESP32's `ledc_ll_set_duty_start` spins inside
// `portENTER_CRITICAL` for up to one carrier period, and no carrier that is slow
// enough for the contactor is fast enough for the 300 ms interrupt watchdog. See
// [`HEATER_LEDC_DEFECT`]; the carrier arithmetic it rests on is in
// `cc_hal_esp32::heater`'s module docs.

/// LEDC at 1 Hz trips the original ESP32's interrupt watchdog, even at duty zero.
///
/// `ledc_ll_set_duty_start` waits for the next timer period inside an interrupt-masked
/// critical section: up to 1 s against a 300 ms watchdog. Raising frequency would
/// increase contactor wear. Use the 10 ms `GPTimer` ISR over a 1000 ms window;
/// see [`cc_hal_esp32::heater`] for the HAL source and carrier constraints.
const HEATER_LEDC_DEFECT: &str =
    "A 1 Hz LEDC carrier spins in ESP-IDF's ledc_ll_set_duty_start for up \
      to one period with interrupts masked, which exceeds the ESP32's 300 ms \
      interrupt watchdog. The spin is unique to this chip -- every other \
      ledc_ll.h in the tree has it removed -- so there is no carrier that is both \
      slow enough for the contactor and fast enough for the watchdog. The heater \
      is driven by a 10 ms GPTimer ISR instead, and no LEDC transport is \
      written: a target whose chip lacks the spin gets one, and the carrier \
      arithmetic survives in cc-hal-esp32::heater's module docs.";

/// Control-task stack sized for credential load/save and nested configuration serde.
///
/// An 8 KB stack overflowed in save with only 68 bytes unused. A 32 KB trial left
/// 23,408 bytes at the deepest point, implying about 8.6 KB use. Keep 16 KB for
/// growth; silent `rst:0xc` resets can mean stack exhaustion, not watchdog expiry.
const CONTROL_STACK_BYTES: usize = 16 * 1024;

/// Stack size of the display task.
///
/// 8 KB. The 1 KB scratch framebuffer is **heap** allocated (`Box`, in
/// `DisplayTask::new`) precisely so that it is not part of this budget; what remains
/// on the stack is the render path and the frame copy.
const DISPLAY_STACK_BYTES: usize = 8 * 1024;

/// Bring-up override for exercising an otherwise disabled scale's fault path.
///
/// Keep false in shipped images so `hardware.sensors.scale.enabled` governs.
/// Without fitted hardware, forcing startup proves only timeout reporting.
/// On hardware (2026-09-29), forcing it on gave 86/138 over-budget ticks; off
/// gave 111/136, with the same 32 ms worst case. Sampling did not cause the overrun.
const BRING_UP_SCALE: bool = false;

/// The startup sequence, in order.
#[allow(
    clippy::too_many_lines,
    reason = "the ordering is the one property that matters here and it is read \
              as a list: the actuators are driven inactive before the sensor, \
              the sensor before the network, and the network before any task \
              exists. Splitting it would hide that."
)]
pub(crate) fn bring_up() -> Result<(), Box<dyn Error>> {
    let peripherals = Peripherals::take()?;

    // The pin map against the wiring, before a single pin is configured.
    //
    // `cc_hal_esp32::pins` holds the numbers every log line below prints; the
    // `peripherals.pins.gpio<NN>` fields are what the machine is actually wired
    // to. Nothing about `esp-idf-hal` 0.47's typed pin fields lets the second
    // be derived from the first without `unsafe` — so this checks, once, and
    // panics with both numbers if they ever disagree. Without it, changing either
    // side silently makes the boot log describe a machine that is not the one that
    // booted, which is the one failure a pin map exists to prevent.
    pins::assert_wiring(&peripherals);
    // The **complete** map, so this one line answers "what is this board
    // actually wired as". Every other pin line below is a subset of it, printed
    // from the same constants.
    info!(
        "pins: heater=GPIO{} valve=GPIO{} pump=GPIO{} switch power=GPIO{} \
         brew=GPIO{} steam=GPIO{} hot_water=GPIO{} tank=GPIO{} \
         temp=GPIO{} i2c sda=GPIO{} scl=GPIO{} scale data=GPIO{}/GPIO{} \
         clock=GPIO{} uart tx=GPIO{} rx=GPIO{}",
        pins::HEATER,
        pins::WATER_VALVE,
        pins::PUMP,
        pins::POWER_SWITCH,
        pins::BREW_SWITCH,
        pins::STEAM_SWITCH,
        pins::WATER_SWITCH,
        pins::WATER_TANK_SENSOR,
        pins::TEMP_SENSOR,
        pins::I2C_SDA,
        pins::I2C_SCL,
        pins::SCALE_DATA_1,
        pins::SCALE_DATA_2,
        pins::SCALE_CLOCK,
        pins::UART_TX,
        pins::UART_RX,
    );

    // The TWDT driver is *moved* into the control task, so the watchdog
    // subscription belongs to the control task and to nothing else.
    let twdt = peripherals.twdt;
    // The LEDC peripheral is deliberately left unused: not taking it at all means
    // no future edit can reach a duty write by accident.
    //
    // `Peripherals` is `#[non_exhaustive]` and must be taken whole, so the field
    // is dropped with it. The comment is the record; the type is 4 bytes.
    let _ = peripherals.ledc;

    // 2. The two plain actuator pins to `inactive`, in `main`, before any task
    //    exists, so there is no window in which a task could observe them
    //    un-driven.
    let water_valve = drive_inactive(peripherals.pins.gpio17, "water valve")?;
    let pump = drive_inactive(peripherals.pins.gpio27, "pump")?;

    // 3. The heater pin, driven **inactive and read back** before anything else
    //    touches it. This is the only moment at which the heater pin can be read
    //    back as a pin: the 10 ms ISR takes ownership of it immediately after,
    //    and from then on the firmware's view of the pin is the ISR's record of
    //    what it drove.
    let heater_pin = drive_inactive(peripherals.pins.gpio2, "heater")?;
    assert!(
        heater_pin.is_low(),
        "startup readback failed: heater is not inactive, refusing to continue"
    );

    // 4. The heater transport: a 10 ms `GPTimer` ISR.
    //
    //    There is no LEDC construction site in this binary and no LEDC transport
    //    type in `cc-hal-esp32`, so there is no path on which a duty write can
    //    reach `ledc_ll_set_duty_start` and its watchdog-eating spin. See
    //    `HEATER_LEDC_DEFECT` above.
    info!("heater: {HEATER_LEDC_DEFECT}");
    // The pin is already inactive and has been read back. `TimerIsrPwm` takes
    // it, configures a 10 ms GPTimer, subscribes the ISR, and starts the timer —
    // with the chopper **disarmed**, so the timer ticks and the pin does not move
    // until the control task opens the gate.
    let transport = TimerIsrPwm::new(heater_pin)?;
    let heater_description = "10 ms GPTimer ISR, disarmed";
    info!(
        "heater: {heater_description}, duty {} ms, window {} ms, counter {}, \
         gate closed until the supervisor beats",
        transport.chopper().duty_ms(),
        transport.chopper().window_ms(),
        transport.chopper().counter_ms(),
    );
    // `max_duty = 1`: the ISR transport works in milliseconds and
    // `HeaterOutput::set_duty` converts the count back through
    // `cc_domain::heater::CHOSEN_MAX_DUTY`, so 1 makes the control task's `set_duty`
    // a pure pass-through of the gate's decision with no second quantisation.
    //
    // **The transport choice is stated here, at its one construction site.** A
    // target whose `ledc_ll.h` has no spin changes this line and adds a transport in
    // `cc-hal-esp32::heater` (`HEATER_LEDC_DEFECT` above).
    let heater = HeaterOutput::new(transport, 1);

    // 5. Read the actuator pins back and assert. A failure here means an actuator
    //    is not in the state the machine considers safe, so nothing else may
    //    proceed.
    let heater_inactive = !heater.transport().is_high();
    for (name, is_inactive) in [
        ("water valve", water_valve.is_low()),
        ("pump", pump.is_low()),
        ("heater", heater_inactive),
    ] {
        assert!(
            is_inactive,
            "startup readback failed: {name} is not inactive, refusing to continue"
        );
    }
    info!(
        "pin readback OK: heater=GPIO{} ({heater_description}) valve=GPIO{} \
         pump=GPIO{} all inactive",
        pins::HEATER,
        pins::WATER_VALVE,
        pins::PUMP,
    );

    // 5b. The scale's pins, if one is configured, and nothing else yet.
    //
    //     The three pins are GPIO32/GPIO25 (data) and GPIO33 (clock), per
    //     `cc_hal_esp32::pins`. They are **inputs and one clock**, so nothing here
    //     can drive an actuator, and they are taken after the actuator readback
    //     above rather than before it, so the readback still covers every pin that
    //     can move a relay.
    //
    //     The configuration is not loaded yet at this point in the sequence, so
    //     whether a scale is fitted is decided below, once `config` exists. What
    //     is done here is the *pin* reservation, which is why the pins are
    //     taken unconditionally and the driver is built later: a driver that
    //     only sometimes exists must not be the thing that decides whether
    //     `Peripherals::take` hands out these pins.
    let scale_pins = ScalePins {
        data_1: peripherals.pins.gpio32,
        data_2: peripherals.pins.gpio25,
        clock: peripherals.pins.gpio33,
    };

    // 5e. The I²C pins, reserved for whichever of the two I²C devices this
    //     machine has fitted.
    //
    //     **The ABP2 and the SSD1306 share one bus** — GPIO21/22, per
    //     `cc_hal_esp32::pins` — and that bus is a mutex-guarded shared resource.
    //     This task takes `I2C0` for the pressure sensor and the display task shares
    //     it. The reservation is here, next to the scale's, so the person who wires
    //     the OLED finds the conflict at the pin stage rather than as a
    //     `Peripherals::take` failure. See the note in `bring_up_pressure`.
    let i2c_pins = I2cPins {
        peripheral: peripherals.i2c0,
        sda: peripherals.pins.gpio21,
        scl: peripherals.pins.gpio22,
    };

    // 5b-bis. The shared I²C bus.
    //
    //     The ABP2 and the SSD1306 are on the same two wires (SCL 22, SDA 21)
    //     and an ESP32 I²C peripheral has exactly one owner, so the bus is put
    //     behind a `Mutex` **once**, here, and both users take it for the length
    //     of one transaction. Neither user may hold it across a frame: a panel
    //     that held the bus would starve the ABP2, and an ABP2 that held it
    //     would make the panel flicker. `display_shared::SharedBus` is the type
    //     that enforces this by only handing out a guard.
    let shared_i2c = match build_shared_i2c(i2c_pins) {
        Ok(bus) => {
            info!(
                "i2c: I2C0 (SDA GPIO{} SCL GPIO{}) at {} kHz, shared between the \
                 ABP2 and the panel",
                pins::I2C_SDA,
                pins::I2C_SCL,
                cc_hal_esp32::sensors::I2C_HZ / 1000,
            );
            Some(bus)
        }
        Err(err) => {
            warn!("i2c: the bus did not come up: {err:?} — no pressure, no display");
            None
        }
    };

    // Move sole actuator ownership into control after inactive readback.
    // Build with safe default polarities; apply stored polarities after config load
    // and before spawning control. Any bring-up inhibit must be explicit and logged.
    let mut actuators = cc_hal_esp32::Actuators::new(
        pump,
        water_valve,
        heater,
        cc_hal_esp32::actuators::RelayPolarities::all_high_trigger(),
    );
    info!(
        "actuators: pump=GPIO{} valve=GPIO{} heater=GPIO{} owned by the control task; \
         no inhibit, the water path follows the reducer",
        pins::PUMP,
        pins::WATER_VALVE,
        pins::HEATER,
    );

    // 5c-bis. The status LED pins, **reserved** here and configured below once
    //     `config` exists.
    //
    //     Reserved for the same reason as the switches' and the scale's: the
    //     `hardware.leds.*.enabled` flags decide whether these pins are driven at
    //     all, and `Peripherals::take` happens exactly once. Taking them here —
    //     unconditionally, before the flags are known — is what makes it
    //     impossible for a flag to decide *which pins the rest of the firmware
    //     gets*, which is the same reasoning `SwitchPins` below records.
    //
    //     **GPIO26 and GPIO19 only.** The steam LED would be GPIO1, and GPIO1
    //     belongs to the UART provisioning console (`start_provisioning` below). A
    //     pin has one owner; see `cc_hal_esp32::pins` for the full reasoning.
    let led_pins = LedPins {
        status: peripherals.pins.gpio26,
        brew: peripherals.pins.gpio19,
    };

    // 5d. The five operator inputs: the four switches and the tank float.
    //
    //     These are **inputs**, so taking them here cannot energise anything, and
    //     the configuration that decides how they are wired is not loaded yet —
    //     so the pins are reserved and `SwitchBank::new` is called below, once
    //     `config` exists. Same shape as the scale's pin reservation above and
    //     for the same reason: `Peripherals::take` happens once, and a driver
    //     that only sometimes exists must not be what decides which pins it
    //     hands out.
    let switch_pins = SwitchPins {
        power: peripherals.pins.gpio39,
        brew: peripherals.pins.gpio34,
        steam: peripherals.pins.gpio35,
        hot_water: peripherals.pins.gpio36,
        water_tank: peripherals.pins.gpio23,
    };

    // ---- R3: storage and the network tier ------------------------------
    //
    // 7. The configuration store, loaded through the fail-closed rule. This is
    //    before the Wi-Fi bring-up because the SSID and the hostname it needs
    //    come out of it, and because the boot log's line about a discarded
    //    configuration is the first thing an operator with a misbehaving machine
    //    needs to see.
    //
    //    Destructured rather than used as a struct: `store` moves into the
    //    control task (the only writer — `BlobConfigStore::load`/`save` take
    //    `&mut self`, and one owner beats a lock), `nvs_description` is the one
    //    thing the HTTP server is given about NVS, and `origin` is printed here
    //    and never needed again.
    let network::Booted {
        config,
        origin,
        mut store,
        nvs_description,
    } = network::bring_up_config()?;
    info!(
        "nvs: {nvs_description} — the predecessor firmware's `config` namespace \
         is a different key space and is ignored by design"
    );
    info!("nvs: the boot decision was `{origin:?}`");

    // **The relay polarities, now that the configuration exists.** `Relay::on()`
    // switches on the configured `hardware.relays.*.trigger_type`, and applying it
    // here is what keeps that parameter honest: a pump or valve relay driven from a
    // hard-coded polarity instead would be **inverted** for a `LOW_TRIGGER` board —
    // `enable_pump` de-energising its relay — and the parameter is reachable over
    // plain `POST /api/parameters`.
    //
    // In practice this reads high-trigger for every relay, because
    // `cc_safety::validate_config` refuses `LOW_TRIGGER` for all of them: such a
    // relay energises while its pin floats, which is before this firmware runs.
    // Applying the configured value is what would carry the day if that refusal were
    // ever relaxed.
    actuators.set_relay_polarities(cc_hal_esp32::actuators::RelayPolarities::from_config(
        &config.hardware.relays,
    ));
    info!(
        "actuators: relay polarity from hardware.relays.*.trigger_type — pump \
         active={:?} valve active={:?}",
        actuators.relay_polarity().pump.active,
        actuators.relay_polarity().valve.active
    );
    // A bench build is only ever one `option_env!` away from being impossible to
    // tell apart from a ship build, so it says so at boot, loudly and unmissably,
    // next to the relay line it is nearest to being unsafe about.
    if cc_domain::BENCH_UNSAFE_TEMPERATURES {
        warn!(
            "SAFETY BENCH BUILD — CC_BENCH_UNSAFE_TEMPERATURES was set when this \
             image was compiled: the emergency-threshold floor and the steam and \
             brew headroom checks are compiled out. DO NOT FIT THIS TO A MACHINE. \
             It exists for operations/runbook.md §13.1."
        );
    }

    // 7a-ter. The status LEDs, now that `hardware.leds.*` is known.
    //
    //     Built here rather than at the pin reservation above for the same reason
    //     the switches' bank is: the `enabled` flags decide whether a pin is
    //     driven, and reading them before the configuration exists would make the
    //     driver — not the operator's setting — the thing that decided.
    //
    //     Each LED is turned **off** as it is built, so a machine powered up
    //     mid-brew does not look idle.
    //
    //     `inverted` comes from `hardware.leds.*.inverted` and **not** from the
    //     relay trigger types: they are separate settings for separate reasons, and
    //     the LEDs' one is about common-anode parts rather than relay coils.
    //
    //     A failure to configure a pin is **not** fatal. `bring_up` returns
    //     `Result`, and an operator whose LED pin is broken should still get a
    //     machine that makes coffee; this is the same call the I²C bus above makes
    //     with `no pressure, no display`.
    let status_pin = led_output_pin(config.hardware.leds.status.enabled, led_pins.status);
    let brew_pin = led_output_pin(config.hardware.leds.brew.enabled, led_pins.brew);
    let leds = cc_hal_esp32::leds::Leds::new(
        status_pin,
        brew_pin,
        config.hardware.leds.status.inverted,
        config.hardware.leds.brew.inverted,
    );
    if leds.any_configured() {
        info!(
            "leds: status=GPIO{} brew=GPIO{} driven by the control task \
             (status inverted={} brew inverted={})",
            cc_hal_esp32::leds::STATUS_PIN,
            cc_hal_esp32::leds::BREW_PIN,
            config.hardware.leds.status.inverted,
            config.hardware.leds.brew.inverted,
        );
    } else {
        info!("leds: hardware.leds.status.enabled and .brew.enabled are both false");
    }
    // The third LED, and the only reason it is missing: the steam LED's pin is
    // GPIO1, which is UART0's TXD and therefore the provisioning console's recovery
    // path. `Peripherals::take` will not hand one pin to two owners, so an operator
    // who left `hardware.leds.steam.enabled` true gets a setting this firmware
    // silently ignores. One `warn!` at boot is the whole diagnostic.
    if config.hardware.leds.steam.enabled {
        warn!(
            "leds: hardware.leds.steam.enabled is set but this firmware drives no steam \
             LED — PIN_STEAMLED (GPIO1) is UART0 TX, reserved for the wifi \
             provisioning console. The steam mode indicator will not light."
        );
    }

    // 7a. The temperature probe, **after the configuration and before anything
    // that reads it**.
    //
    //     `hardware.sensors.temperature.type` selects the driver, so the probe
    //     cannot be built before that value is known. The ordering that matters for
    //     safety is untouched: the actuators were driven inactive and the heater read
    //     back above, before this line.
    //
    //     GPIO16 is `pins::TEMP_SENSOR` and whichever driver is selected is built on
    //     it. This is a **read**: the 1-Wire bus is open-drain and the ZACwire line
    //     is an input, so nothing on this pin is ever energised.
    let temp_sensor = bring_up_temperature_sensor(peripherals.pins.gpio16, &config)?;

    // 7b. The scale, **constructed at boot**.
    //
    //     The branch below is real: the driver is built, its sampling task started,
    //     and the weight reaches the telemetry the display and MQTT read.
    //
    //     The type selects one cell or two (`hardware.sensors.scale.type`,
    //     `cc_domain::hardware::ScaleType`), which is what makes that setting do
    //     something. A `Bluetooth` type is refused here rather than silently
    //     reporting zero grams.
    let sampler = match bring_up_scale(scale_pins, &config, &mut store) {
        Ok(sampler) => sampler,
        Err(err) => {
            // Not fatal. A machine whose scale will not start must still run its
            // control loop, and the absence is visible in `/api/status` as a
            // null weight rather than as a machine that will not boot.
            warn!("scale: not started — {err}. The weight will be reported absent.");
            None
        }
    };
    if let Some(sampler) = sampler.as_ref() {
        info!(
            "scale: hardware.sensors.scale.type is {:?}, enabled — {}",
            config.hardware.sensors.scale.r#type,
            sampler.telemetry().describe(),
        );
    } else {
        info!(
            "scale: hardware.sensors.scale.enabled is {} — not fitted",
            config.hardware.sensors.scale.enabled,
        );
    }

    // 7c. The five operator inputs. Built here because this is the first point
    //     at which `config` exists, and the switch's type and mode come from it
    //     (`hardware.switches.*.type` / `.mode`).
    //
    //     A failure here is **fatal**, unlike the scale's. A scale that will not
    //     start costs a weight reading; a switch bank that will not configure costs
    //     every physical control the machine has, and the machine would sit there
    //     accepting web commands it could not be overridden on.
    let switches = SwitchBank::new(
        switch_pins.power,
        switch_pins.brew,
        switch_pins.steam,
        switch_pins.hot_water,
        switch_pins.water_tank,
        &config,
    )?;

    // Optional ABP2 polling uses conversion deadlines, never sleeps in the tick.
    // Move only the boxed bus into the static task; construct its borrowers there
    // to avoid a self-referential bundle. Bus failure leaves the panel and pressure
    // sensor absent without stopping control.
    let shared_i2c: Option<Box<cc_hal_esp32::display_shared::SharedBus>> = shared_i2c.map(Box::new);

    // 8. The shared HTTP state and the network→control command queue.
    let net = Arc::new(network::Network::new());
    let commands = Arc::new(cc_hal_esp32::task::CommandQueue::new());

    // 9. Wi-Fi. Brought up only when a network is configured: the netif is
    //    created with the hostname on it (the ordering `WiFiStaConnect.h` exists
    //    to protect) and the monitor takes it from there.
    // Held for the life of `main` so the netif is never torn down while the httpd
    // task is serving `/api/status`.
    // 8b. The network stack, **before** the radio and before the HTTP server and
    //     unconditionally. `esp_netif_init` and `esp_event_loop_create_default`
    //     are once-per-process, and everything that opens a socket needs them —
    //     including the HTTP server, which does not care whether a radio
    //     exists. An unprovisioned machine must still serve `/api/status` and a
    //     telnet console, or there is no way to find out why it is unprovisioned.
    let sys_loop = cc_hal_esp32::wifi::init_stack()?;
    info!("netif: lwIP and the default event loop are up");

    let wifi = if config.is_wifi_provisioned() {
        Some(bring_up_wifi(peripherals.modem, &config, &sys_loop)?)
    } else {
        info!("wifi: no SSID configured — the UART provisioning task will run");
        None
    };

    // 10. The HTTP server. `EspHttpServer` is neither `Send` nor `Sync`, so it
    //     lives in this frame — and `main` blocks on `control.join()` for the
    //     life of the process, so its `Drop` (which stops the httpd task) never
    //     runs. See `network::start_http`.
    //
    //     The parameter mailbox is created here, beside the command queue, because
    //     it is the same seam: two producers' worth of request, one consumer, and
    //     the control task drains both at the top of its tick.
    let parameters = Arc::new(cc_hal_esp32::task::ParameterHandoff::new());
    let _http = network::start_http(
        &net,
        &config,
        &nvs_description,
        Arc::clone(&commands),
        &parameters,
    )?;

    // 10a. The telnet log stream, which `docs/operations/runbook.md` documents as
    //     the field-diagnosis path. Spawned here, beside the HTTP server and after
    //     it, because the listener is a network-tier service with no bearing on the
    //     control loop: a failure to bind is a `warn!` inside the task rather than a
    //     boot failure.
    //
    //     It is at `task::TELNET_PRIO` = 2, the lowest priority in the firmware:
    //     a terminal that has stopped reading costs the ring a dropped line and
    //     nothing else, and the ring push is a bounded copy with no allocation.
    if let Err(err) = cc_hal_esp32::telnet::start() {
        error!("telnet: the log stream task could not be spawned: {err}");
    }

    // 11b. The UART provisioning task. It always starts, and it still **exits
    //     after success** — it is not a permanent task.
    //
    //     Starting it only when no credential is stored made a **wrong** network
    //     unfixable over the cable: with a credential stored the console never
    //     armed, and the only way to change it was `POST /api/wifi-reset`, which
    //     needs a machine that is already online. Found on the bench with the machine
    //     configured for a network that did not exist and no way to point it at one
    //     that did. `wifi set` + `wifi apply` replaces whatever is stored, and the
    //     task costs one UART poll loop.
    //
    //     The handoff is shared with the control task, which does the writing: the
    //     store moved into that task in step 7, and a credential cannot be stored by
    //     a task that does not hold the store.
    let handoff = network::Handoff::new();
    if config.is_wifi_provisioned() {
        info!(
            "wifi: a credential is stored — the provisioning console is still \
             available, and `wifi set` + `wifi apply` will replace it"
        );
    }
    start_provisioning(
        peripherals.uart0,
        peripherals.pins.gpio1,
        peripherals.pins.gpio3,
        handoff.clone(),
    );

    // 11. Whether MQTT is configured, and nothing else. The **client** is built
    //     inside the control task, not here.
    //
    //     A client built here and read two lines later would be dropped at the end
    //     of this `match` arm, and `impl Drop for EspMqttClient` calls
    //     `esp_mqtt_client_destroy` (`esp-idf-svc` `src/mqtt/client.rs:742-747`).
    //     Destruction microseconds after `esp_mqtt_client_start` is a client that can
    //     never publish or subscribe.
    //
    //     It lives in the control task because that task is the main loop: it owns
    //     the clock, the configuration, the store, the machine, and the only watchdog
    //     subscription, so a publish that stalls is visible. See `mqtt_link`.
    //
    //     `mqtt.enabled` is false and `mqtt.broker` is empty by default, so an
    //     unprovisioned machine spends nothing here: no 4 KB of task stack and no two
    //     1 KB buffers for a client with nowhere to connect.
    let mqtt_configured = cc_hal_esp32::mqtt::is_configured(&config);
    if !mqtt_configured {
        info!("mqtt: not configured (mqtt.enabled is false or mqtt.broker is empty)");
    }

    // Control owns actuators, watchdog subscription, deadman, and config writes.
    // Its config is authoritative; the HTTP server's boot snapshot can remain stale
    // until reboot unless a live publication replaces it.
    let control_config = config.clone();

    // ---- the two peer tasks, and the channels between the three -------------
    //
    // `switches` and `temp` move **out** of the control task and into the sensor
    // task, and the panel moves out into the display task. What stays here is
    // the wiring: one [`slots::SensorSlots`] for the readings, the edges and the
    // frame request, and one [`cc_hal_esp32::task::SignalQueue`] the producers
    // use to wake the consumer.
    //
    // The bus is leaked deliberately. It was a `Box` owned by the control task's
    // frame so its two users could hold `&'static` references into it; now two
    // *different* tasks need one, and a self-referential bundle holding a
    // reference into itself is not constructible. A `Box::leak` is the honest
    // spelling of what was always true — the bus lives for the whole program,
    // because all three of its users run forever.
    let frame = Arc::new(slots::FrameSlot::new());

    // The I²C bus, leaked. It has two users in **different** tasks — the ABP2 in
    // the control task and the panel in the display task — and both want a
    // `&'static` reference into it, which a `Box` owned by one task cannot
    // honestly give the other. A `Box::leak` says what was always true: the bus
    // lives for the whole program, because both of its users run until the
    // process ends. One bounded leak, documented, beats a lifetime lie.
    let shared_i2c: Option<&'static cc_hal_esp32::display_shared::SharedBus> =
        shared_i2c.map(|bus| &*Box::leak(bus));

    // The sensor task settles the switches and publishes where they rest. The
    // control task waits for that before it boots: the power switch decides between
    // `PID_NORMAL` and `PID_DISABLED`.
    // The panel, for the display task. Boxed, for the same reason `ControlArgs`
    // is: `bring_up` runs on ESP-IDF's main task, and a task bundle built *by
    // value* in this frame is a frame cost this function cannot see.
    let display_template = display_task::template_for(config.display.template);
    // `hardware.oled.enabled` gates the **panel**, never the bus: with the flag off
    // this does not call `bring_up` at all, so no `INIT_SEQUENCE`, no address probe
    // and no traffic on the bus the pressure sensor shares. That is also the cheaper
    // reading on that bus.
    //
    // **Not** `set_blank(true)`: blanking is the *standby* mechanism
    // (`machine.standby.should_turn_off_display()`), it keeps the controller
    // alive so waking is free, and it does not stop the boot screens being
    // drawn. "Off" has to mean the panel is never touched.
    let panel = if config.hardware.oled.enabled {
        shared_i2c.map(cc_hal_esp32::display_shared::SharedPanel::bring_up)
    } else {
        info!("display: hardware.oled.enabled is false — the panel is not brought up");
        None
    };
    let display = Box::new(display_task::DisplayTask::new(
        panel,
        display_template,
        Arc::clone(&frame),
        Arc::clone(&net.shared),
    ));

    // The radio moves into the control task rather than staying in this frame.
    // It is `Send` (`EspWifi` is, and `Monitor` and `String` are), and the
    // control task is the only task with a watchdog subscription, so the 1 s
    // radio poll and the `/api/status` radio fields
    // (`network::publish_radio`) both belong to the task whose stalls are already
    // fatal. Before this, the radio was polled nowhere and `/api/status`
    // reported `wifiAssociated: false` on a machine that was associated.
    //
    // **The box is built here, in `bring_up`'s 16 KB frame**, and the closure
    // carries only a pointer. See `ControlArgs` for why that matters: a by-value
    // argument is materialised in the caller's frame, and the caller here *is*
    // the 8 KB control stack.
    let args = Box::new(control_task::ControlArgs {
        actuators,
        leds,
        switches,
        shared_i2c,
        temp: temp_sensor,
        net: Arc::clone(&net),
        commands: Arc::clone(&commands),
        frame: Arc::clone(&frame),
        parameters,
        config: control_config,
        mqtt_configured,
        store,
        sampler,
        handoff: handoff.clone(),
        sta: wifi,
    });
    let control = cc_hal_esp32::task::spawn_with_prio(
        c"control",
        CONTROL_STACK_BYTES,
        cc_hal_esp32::task::CONTROL_PRIO,
        move || {
            if let Err(err) = control_task::run(twdt, args) {
                error!("control task failed: {err}");
            }
        },
    )?;

    // The two peers, spawned after the control task so the watchdog — which the
    // control task owns and which panics the chip when it trips — is already
    // subscribed before anything else starts moving.
    //
    // **The display task and nothing else.** A sensor task was measured and
    // removed. The bisect is in `sensor_task.rs`. Recheck 2027-01.
    let display_thread = cc_hal_esp32::task::spawn_with_prio(
        c"display",
        DISPLAY_STACK_BYTES,
        cc_hal_esp32::task::DISPLAY_PRIO,
        move || display.run(),
    );
    if let Err(err) = display_thread {
        error!("display task could not be spawned: {err}");
    }

    // The control task is the only feed point, so `main` must not return while
    // it is alive. A panic inside it is fatal and is reported, not swallowed.
    control.join().map_err(|_| "control task panicked")?;

    Ok(())
}

/// Log where the switches are resting, once, at boot.
///
/// The four operator switches are **floating inputs** — GPIO34/35/36/39 have no
/// internal pull, and the driver asks ESP-IDF for `IN_HARDWARE`. On a board where a
/// switch is not wired a floating pin wanders and the debouncer will eventually
/// report a press. The switches are enabled by default, so this line is what answers
/// "is my board about to start a brew by itself?" — and it must say the level, not
/// just that the switch exists.
pub(crate) fn log_switch_levels(levels: cc_hal_esp32::switches::Levels) {
    info!(
        "switch resting levels after settling: power={} brew={} steam={} \
         hot_water={} water_tank={} -- a floating input reading high here with no \
         switch wired will eventually read as a press",
        levels.power, levels.brew, levels.steam, levels.hot_water, levels.water_tank_full,
    );
}

/// Drives one actuator pin to the inactive level and returns the driver.
///
/// `input_output` rather than `output`, because the readback assertion needs to
/// observe the pin: in `esp-idf-hal` 0.47 `is_low()` is bounded on `InputMode`,
/// which `Output` does not implement (see `gpio.rs:362-380`).
fn drive_inactive<'d, P>(pin: P, name: &str) -> Result<PinDriver<'d, InputOutput>, EspError>
where
    P: InputPin + OutputPin + 'd,
{
    let mut driver = PinDriver::input_output(pin, Pull::Floating)?;
    driver.set_level(INACTIVE)?;
    info!("{name} driven inactive");
    Ok(driver)
}

/// The five operator inputs, taken from `Peripherals` and held until the
/// configuration says how they are wired.
///
/// Same shape and same reason as [`ScalePins`]: the pins are reserved before the
/// configuration is loaded, and the driver is built after. `SwitchBank::new`
/// takes them by value and returns the bank, so there is one owner of each pin
/// from `Peripherals::take()` onwards.
struct SwitchPins {
    /// `PIN_POWERSWITCH` (GPIO39).
    power: esp_idf_hal::gpio::Gpio39<'static>,
    /// `PIN_BREWSWITCH` (GPIO34).
    brew: esp_idf_hal::gpio::Gpio34<'static>,
    /// `PIN_STEAMSWITCH` (GPIO35).
    steam: esp_idf_hal::gpio::Gpio35<'static>,
    /// `PIN_WATERSWITCH` (GPIO36) — the hot-water button.
    hot_water: esp_idf_hal::gpio::Gpio36<'static>,
    /// `PIN_WATERTANKSENSOR` (GPIO23) — the tank float.
    water_tank: esp_idf_hal::gpio::Gpio23<'static>,
}

/// The two status LED pins, taken from `Peripherals` at bring-up and held until
/// `hardware.leds.*.enabled` says whether they are driven.
///
/// GPIO26 and GPIO19, and **no third field**. GPIO1 belongs to the provisioning
/// console's UART0 TXD, and `Peripherals::take` will not hand one pin to two owners.
/// The full analysis, including what moving the steam LED to GPIO32 would cost, is
/// the comment block in `cc_hal_esp32::pins`.
struct LedPins {
    /// The status LED, on GPIO26.
    status: esp_idf_hal::gpio::Gpio26<'static>,
    /// The brew LED, on GPIO19.
    brew: esp_idf_hal::gpio::Gpio19<'static>,
}

/// Configure one LED pin as a push-pull output, or `None` when the operator has
/// the LED disabled.
///
/// The `enabled` gate is here, not at the call site, so that "no LED" is always
/// `None` and never a pin somebody could still write. A disabled LED's pin is simply
/// dropped — `led_pins`' field is moved in and gone.
///
/// # Errors
///
/// `EspError` from [`esp_idf_hal::gpio::PinDriver::output`], which fails if the
/// pin is already owned. A machine that hits this has a wiring fault; the caller
/// logs it and runs without the LED rather than refusing to boot, because
/// "the brew LED does not work" is not a reason a coffee machine makes no
/// coffee.
fn led_output_pin<P>(enabled: bool, pin: P) -> Option<PinDriver<'static, esp_idf_hal::gpio::Output>>
where
    P: esp_idf_hal::gpio::OutputPin + 'static,
{
    if !enabled {
        return None;
    }
    match esp_idf_hal::gpio::PinDriver::output(pin) {
        Ok(driver) => Some(driver),
        Err(err) => {
            warn!("leds: a status LED pin could not be configured as an output: {err:?}");
            None
        }
    }
}

/// The shared I²C bus, taken from `Peripherals` and held until the ABP2 is built.
///
/// `I2C0` plus its two pins — GPIO21/22 for **both** devices — so whichever of the two
/// lands second has to share the driver. The bus needs a mutex, which does not exist
/// yet, so **this function takes the bus for the pressure sensor** and the display
/// task must either share this driver (`Abp2I2c::from_driver` exists for exactly
/// that) or wait.
struct I2cPins {
    /// `I2C0`, the only I²C peripheral this build uses.
    peripheral: esp_idf_hal::i2c::I2C0<'static>,
    /// `PIN_I2CSDA` (GPIO21).
    sda: esp_idf_hal::gpio::Gpio21<'static>,
    /// `PIN_I2CSCL` (GPIO22).
    scl: esp_idf_hal::gpio::Gpio22<'static>,
}

/// Build the ABP2 pressure sensor, when one is fitted.
///
/// `hardware.sensors.pressure.enabled` is the gate, and it defaults to `false`, so a
/// machine with no ABP2 spends no bus, no driver and no 20 Hz deadline on one — and
/// `/api/status` publishes `pressure: null`.
///
/// A failure is **not** fatal. A pressure sensor that does not answer costs a
/// telemetry field, not the machine; refusing to boot over one would be a regression.
///
/// # How the 10 ms is spent
///
/// A conversion needs 10 ms inside a 50 ms cadence — **20 % of a loop period**.
/// `cc_protocol::abp2::Driver` makes that a *deadline* instead of a delay: the
/// conversion command is written on one tick and the answer read on a later one, so
/// the sensor costs two I²C transactions spread across normal control-loop work and no
/// sleep at all.
/// Construct the I²C bus once and wrap it for sharing.
///
/// Deliberately separate from [`bring_up_pressure`]: the pressure sensor and
/// the panel are peers on this bus, and a helper that handed the bus to one of
/// them would be the bug this restructure exists to remove.
fn build_shared_i2c(
    pins: I2cPins,
) -> Result<cc_hal_esp32::display_shared::SharedBus, esp_idf_svc::sys::EspError> {
    let sda: cc_hal_esp32::sensors::SdaPin = pins.sda.into();
    let scl: cc_hal_esp32::sensors::SclPin = pins.scl.into();
    let bus = cc_hal_esp32::Abp2I2c::new(pins.peripheral, sda, scl)?;
    // `into_driver`: the `Abp2I2c` facade is a thin wrapper whose only job was
    // to own the bus, and the shared type owns it now.
    Ok(cc_hal_esp32::display_shared::SharedBus::new(
        bus.into_inner(),
    ))
}

pub(crate) fn bring_up_pressure<'bus>(
    bus: Option<&'bus cc_hal_esp32::display_shared::SharedBus>,
    config: &cc_config::Config,
) -> Option<cc_hal_esp32::Abp2Pressure<&'bus cc_hal_esp32::display_shared::SharedBus>> {
    if !config.hardware.sensors.pressure.enabled {
        info!("pressure: hardware.sensors.pressure.enabled is false — no ABP2");
        return None;
    }
    // The bus is shared with the panel, so the sensor borrows it rather than
    // owning it: it takes the lock for one transaction and gives it straight
    // back. `None` here means the bus itself failed to come up in `bring_up`.
    let bus = bus?;
    info!(
        "pressure: ABP2 on the shared I2C0 (SDA GPIO{} SCL GPIO{}) at 0x{:02X}, \
         non-blocking — the 10 ms conversion is a deadline here, cadence {} ms",
        pins::I2C_SDA,
        pins::I2C_SCL,
        cc_protocol::abp2::ADDRESS,
        cc_protocol::abp2::CADENCE.raw(),
    );
    Some(cc_hal_esp32::Abp2Pressure::on_shared_bus(bus))
}

/// The three scale pins, taken from `Peripherals` and held until the driver is
/// built.
///
/// A named struct rather than three locals because the pins are taken in one
/// place and used in another: `bring_up` reserves them unconditionally — so
/// that the actuator readback above covers every pin before any of them is
/// spoken for — and the driver is built later, once the configuration says
/// whether a scale is fitted.
struct ScalePins {
    /// `PIN_HXDAT`, the first data line.
    data_1: esp_idf_hal::gpio::Gpio32<'static>,
    /// `PIN_HXDAT2`, the second data line, on a dual scale.
    data_2: esp_idf_hal::gpio::Gpio25<'static>,
    /// `PIN_HXSCK`, the clock, shared by both cells.
    clock: esp_idf_hal::gpio::Gpio33<'static>,
}

/// Build the scale driver and start its sampling task.
///
/// Returns `Ok(None)` when `hardware.sensors.scale.enabled` is `false` — which
/// is the **default** (`cc-config`, `HardwareSensorsScale::default`) and is not
/// an error: a machine with no scale must not spend a task, a stack and a
/// priority level on one. The `hardware.sensors.scale.*` settings are read here
/// and nowhere else, which is what makes them do something rather than describe
/// a feature that does not exist.
///
/// # Errors
///
/// [`EspError`] if the pins cannot be configured or the task cannot be created.
/// The caller treats this as "the machine runs without a scale and says so".
fn bring_up_scale(
    pins: ScalePins,
    config: &cc_config::Config,
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
) -> Result<Option<cc_hal_esp32::Sampler>, EspError> {
    let scale_config = &config.hardware.sensors.scale;
    // `hardware.sensors.scale.enabled` decides, **overridden by
    // `BRING_UP_SCALE`**. See that constant for why the override exists and
    // what it is not.
    if !scale_config.enabled && !BRING_UP_SCALE {
        return Ok(None);
    }
    if !scale_config.enabled {
        warn!(
            "scale: hardware.sensors.scale.enabled is false and BRING_UP_SCALE \
             is on — starting the driver anyway. This is a bring-up build: the \
             weight will read as absent because no scale is fitted, and that is \
             the fault path being exercised, not a working scale."
        );
    }

    // 128 gain steps, which is what `cc_protocol::sensor::hx711::Rate` names as its
    // default and what the HX711 powers up with.
    let rate = cc_protocol::sensor::hx711::Rate::default();

    // `samples` is an `i32` config parameter in `1..=20`; anything else in a
    // blob is clamped by the domain crate rather than rejected, because
    // refusing to sample would stop the scale entirely.
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "the config range is 1..=20 (`cc-config`) and the domain crate \
                  clamps again; a corrupt blob must not stop the scale, and a \
                  negative one must not be read as a huge positive"
    )]
    let average = scale_config.samples.clamp(1, i32::from(u8::MAX)) as u8;

    // 🔴 `hardware.sensors.scale.type` selects one cell or two, and this is where that
    // setting becomes real.
    //
    // The two HX711 arms are the same shape — configure a bus, build a driver — and
    // differ only in how many data lines, so they are built first and paired
    // afterwards. `Bluetooth` is not an HX711 at all and returns before either is
    // built.
    let (bus, driver) = match scale_config.r#type {
        cc_domain::hardware::ScaleType::Hx711Dual => {
            let bus = cc_hal_esp32::GpioHx711::dual(pins.data_1, pins.data_2, pins.clock)?;
            let driver = cc_protocol::sensor::hx711::Scale::dual(
                scale_config.calibration,
                scale_config.calibration2,
                average,
            );
            (bus, driver)
        }
        cc_domain::hardware::ScaleType::Hx711Single => {
            let bus = cc_hal_esp32::GpioHx711::single(pins.data_1, pins.clock)?;
            let driver =
                cc_protocol::sensor::hx711::Scale::single(scale_config.calibration, average);
            (bus, driver)
        }
        // A Bluetooth scale is a different driver on a different transport, reading a
        // different device. Refused here, explicitly and by name, rather than quietly
        // reporting 0 g — which reads as "the scale is working" and measures nothing.
        // Naming the variant rather than `_` is deliberate: a fourth `ScaleType` then
        // becomes a compile error here instead of a fourth silently-refused case.
        other @ cc_domain::hardware::ScaleType::Bluetooth => {
            warn!(
                "scale: hardware.sensors.scale.type is {other:?}, which is an Acaia \
                 BLE scale and not an HX711 — refused. The weight is reported \
                 absent rather than as 0 g."
            );
            return Ok(None);
        }
    };

    // 🔴 The stored tare, handed to the sampler before its first reading.
    //
    // A tare in NVS is only useful if it is applied *before* the first weight
    // is published, or the machine briefly reports the un-tared offset — which
    // on a 267 g default known weight is a cup of coffee's worth of phantom
    // weight in `/api/status` and on the display. So this is read here and
    // queued immediately, before the task's first pass can publish anything.
    match cc_hal_esp32::nvs::load_tare(store.backend()) {
        Ok(Some(record)) => info!(
            "scale: a stored tare was found — cell 1 offset {}, cell 2 offset {}",
            record.offset_1, record.offset_2
        ),
        Ok(None) => info!("scale: no stored tare — the scale will tare at start-up"),
        Err(err) => warn!("scale: the stored tare could not be read ({err}) — taring at start-up"),
    }
    let stored_tare = cc_hal_esp32::nvs::load_tare(store.backend()).ok().flatten();

    let telemetry = Arc::new(cc_hal_esp32::scale::Telemetry::new());
    let sampler = cc_hal_esp32::Sampler::start(bus, driver, rate, Arc::clone(&telemetry))?;
    if let Some(record) = stored_tare {
        // A record the sampler refuses is a warning there, not an error here:
        // it will tare for itself, which is the correct outcome for a tare that
        // does not describe this scale. A `false` here means the command queue
        // was full, which at boot it cannot be.
        if !sampler.request_restore(record) {
            warn!("scale: the stored tare could not be handed to the sampler");
        }
    }
    Ok(Some(sampler))
}

/// Spawn the UART provisioning task on UART0.
///
/// A failure to install the UART driver is **not** fatal — it warns and returns, and
/// the machine runs with no network surface rather than refusing to boot.
fn start_provisioning(
    uart: esp_idf_hal::uart::UART0<'static>,
    tx: esp_idf_hal::gpio::Gpio1<'static>,
    rx: esp_idf_hal::gpio::Gpio3<'static>,
    handoff: network::Handoff,
) {
    let serial = match cc_hal_esp32::provisioning::Serial::new(uart, tx, rx) {
        Ok(serial) => serial,
        Err(err) => {
            warn!("serial: the UART driver did not install: {err:?} -- provisioning is off");
            return;
        }
    };
    let spawned = cc_hal_esp32::task::spawn_with_prio(
        c"provision",
        network::PROVISION_THREAD_STACK_BYTES,
        cc_hal_esp32::task::PROVISION_PRIO,
        move || network::run_provisioning(serial, handoff),
    );
    if let Err(err) = spawned {
        warn!("serial: the provisioning task did not start: {err}");
    }
}

/// Bring the station interface up and associate.
///
/// # Errors
///
/// [`EspError`] from the netif, the driver, or the association itself. The
/// caller treats a failure as "the machine runs offline", which is what a
/// machine with no radio does.
fn bring_up_wifi(
    modem: esp_idf_hal::modem::Modem<'static>,
    config: &cc_config::Config,
    sys_loop: &esp_idf_svc::eventloop::EspSystemEventLoop,
) -> Result<cc_hal_esp32::Sta, EspError> {
    // The hostname goes in with the netif, before `wifi.start()`, so the DHCP
    // client identifier is fixed before the first association. See
    // `cc_hal_esp32::wifi`'s module documentation.
    let mut sta = cc_hal_esp32::Sta::new(modem, &config.system.hostname, sys_loop)?;
    // **Lengths only, never the values.** A machine that will not associate is
    // almost always holding a credential that is not the one it thinks it has, and
    // "the stored SSID is 12 bytes" settles a class of problem that reading
    // `/api/parameters` cannot reach when the machine is offline. `password` is a
    // `Secret` and is exposed only for its length here.
    // The SSID is not a secret — it is already in every boot log — so it is printed
    // to say *which* network is configured, which is the thing a length cannot tell
    // you. The password is never printed at all, only its length.
    info!(
        "wifi: stored credential — ssid {:?} ({} bytes), password {} bytes",
        config.system.wifi.ssid,
        config.system.wifi.ssid.len(),
        config.system.wifi.password.expose().len(),
    );
    sta.connect(
        &config.system.wifi.ssid,
        config.system.wifi.password.expose(),
    )?;
    if sta.wait_for_connection() {
        info!("wifi: associated, {}", sta.describe());
    } else {
        // An explicit SSID that does not associate means offline mode, no portal.
        warn!("wifi: the configured network is unavailable — running offline");
        sta.leave_offline();
    }
    Ok(sta)
}
