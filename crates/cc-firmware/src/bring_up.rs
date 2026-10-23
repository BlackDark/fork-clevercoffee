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

/// The startup sequence. The order is the safety property: the actuators are
/// driven inactive and read back before the configuration is loaded, the
/// configuration before any peripheral that depends on it, and the network before
/// any task exists.
pub(crate) fn bring_up() -> Result<(), Box<dyn Error>> {
    let peripherals = Peripherals::take()?;
    // The pin map against the wiring, before any pin is configured: every number
    // the log prints comes from `pins`, and this panics if the two disagree.
    pins::assert_wiring(&peripherals);
    log_pin_map();

    // Moved into the control task, so the watchdog subscription is that task's.
    let twdt = peripherals.twdt;
    // Not taken at all, so no edit can reach an LEDC duty write by accident.
    // `Peripherals` is `#[non_exhaustive]` and must be taken whole.
    let _ = peripherals.ledc;

    let mut actuators = secure_actuators(
        peripherals.pins.gpio17,
        peripherals.pins.gpio27,
        peripherals.pins.gpio2,
    )?;

    // Pins are reserved now, unconditionally, and their drivers built once the
    // configuration says how: a configuration flag must never decide which pins
    // the rest of the firmware gets from the one `Peripherals::take`. All are
    // inputs or LEDs, taken after the readback, so none can move a relay.
    let scale_pins = ScalePins {
        data_1: peripherals.pins.gpio32,
        data_2: peripherals.pins.gpio25,
        clock: peripherals.pins.gpio33,
    };
    // The ABP2 and the SSD1306 share this bus.
    let shared_i2c = shared_bus(I2cPins {
        peripheral: peripherals.i2c0,
        sda: peripherals.pins.gpio21,
        scl: peripherals.pins.gpio22,
    });
    // GPIO26 and GPIO19 only: the steam LED would be GPIO1, the console's TXD.
    let led_pins = LedPins {
        status: peripherals.pins.gpio26,
        brew: peripherals.pins.gpio19,
    };
    let switch_pins = SwitchPins {
        power: peripherals.pins.gpio39,
        brew: peripherals.pins.gpio34,
        steam: peripherals.pins.gpio35,
        hot_water: peripherals.pins.gpio36,
        water_tank: peripherals.pins.gpio23,
    };

    let network::Booted {
        config,
        mut store,
        nvs_description,
        ..
    } = load_config()?;

    apply_relay_polarities(&mut actuators, &config);
    let leds = bring_up_leds(led_pins, &config);
    // After the configuration, which selects the driver; a read-only pin.
    let temp_sensor = bring_up_temperature_sensor(peripherals.pins.gpio16, &config)?;
    let sampler = start_scale(scale_pins, &config, &mut store);
    // Fatal, unlike the scale: without the switches the machine accepts web
    // commands it cannot be overridden on.
    let switches = SwitchBank::new(
        switch_pins.power,
        switch_pins.brew,
        switch_pins.steam,
        switch_pins.hot_water,
        switch_pins.water_tank,
        &config,
    )?;

    let tier = start_network_tier(
        peripherals.modem,
        peripherals.uart0,
        peripherals.pins.gpio1,
        peripherals.pins.gpio3,
        &config,
        &nvs_description,
    )?;

    // Leaked: its two users run in different tasks (the ABP2 in control, the
    // panel in display) and both need `&'static`. It lives for the whole program
    // because both users do; a leak says so where a borrow would lie.
    let shared_i2c: Option<&'static cc_hal_esp32::display_shared::SharedBus> =
        shared_i2c.map(|bus| &*Box::leak(Box::new(bus)));

    let frame = Arc::new(slots::FrameSlot::new());
    // Boxed: the closure then carries a pointer, not the task.
    let display = Box::new(display_task_for(&config, shared_i2c, &frame, &tier.net));

    // Built here, in this 16 KB frame, so the closure carries a pointer; see
    // `ControlArgs` for why a by-value bundle overflowed the control stack.
    let args = Box::new(control_task::ControlArgs {
        actuators,
        leds,
        switches,
        shared_i2c,
        temp: temp_sensor,
        net: Arc::clone(&tier.net),
        commands: Arc::clone(&tier.commands),
        frame,
        parameters: Arc::clone(&tier.parameters),
        // Authoritative from here; the HTTP server's boot copy goes stale until a
        // live publication replaces it.
        config,
        mqtt_configured: tier.mqtt_configured,
        store,
        sampler,
        handoff: tier.handoff.clone(),
        // `Send`, and polled by the only task whose stalls are already fatal.
        sta: tier.sta,
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

    // After the control task, so the watchdog is subscribed before anything else
    // moves. There is no sensor task: see `sensor_task.rs` (recheck 2027-01).
    spawn_display(display);

    // `tier` holds the HTTP server and the event loop in this frame, which does
    // not return while the control task runs: their `Drop`s would stop the httpd
    // task and delete the default event loop.
    control.join().map_err(|_| "control task panicked")?;
    Ok(())
}

/// The configuration, through the fail-closed rule.
///
/// Before Wi-Fi: the SSID and hostname come out of it, and a discarded
/// configuration is the first thing an operator with a misbehaving machine reads.
fn load_config() -> Result<network::Booted, EspError> {
    let booted = network::bring_up_config()?;
    info!(
        "nvs: {} — the predecessor firmware's `config` namespace is a different key \
         space and is ignored by design",
        booted.nvs_description
    );
    info!("nvs: the boot decision was `{:?}`", booted.origin);
    Ok(booted)
}

/// Start the display task. A failure costs the panel, not the machine.
fn spawn_display(display: Box<display_task::DisplayTask>) {
    let spawned = cc_hal_esp32::task::spawn_with_prio(
        c"display",
        DISPLAY_STACK_BYTES,
        cc_hal_esp32::task::DISPLAY_PRIO,
        move || display.run(),
    );
    if let Err(err) = spawned {
        error!("display task could not be spawned: {err}");
    }
}

/// The complete pin map in one line: "what is this board wired as".
fn log_pin_map() {
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
}

/// Drive the three actuator pins inactive, read each back, and hand them to the
/// actuator facade. Panics on a failed readback: nothing may run with an actuator
/// in a state the machine does not consider safe.
///
/// The heater is read back as a pin only here; the 10 ms ISR owns it from then
/// on, starting **disarmed** until the control task opens the gate. There is no
/// LEDC transport anywhere (see [`HEATER_LEDC_DEFECT`]).
///
/// The facade starts high-trigger; [`apply_relay_polarities`] sets the configured
/// polarity once the configuration exists.
fn secure_actuators(
    water_valve: esp_idf_hal::gpio::Gpio17<'static>,
    pump: esp_idf_hal::gpio::Gpio27<'static>,
    heater: esp_idf_hal::gpio::Gpio2<'static>,
) -> Result<cc_hal_esp32::Actuators, EspError> {
    let water_valve = drive_inactive(water_valve, "water valve")?;
    let pump = drive_inactive(pump, "pump")?;
    let heater_pin = drive_inactive(heater, "heater")?;
    assert!(
        heater_pin.is_low(),
        "startup readback failed: heater is not inactive, refusing to continue"
    );

    info!("heater: {HEATER_LEDC_DEFECT}");
    let transport = TimerIsrPwm::new(heater_pin)?;
    let heater_description = "10 ms GPTimer ISR, disarmed";
    info!(
        "heater: {heater_description}, duty {} ms, window {} ms, counter {}, \
         gate closed until the supervisor beats",
        transport.chopper().duty_ms(),
        transport.chopper().window_ms(),
        transport.chopper().counter_ms(),
    );
    // `max_duty = 1`: the ISR works in milliseconds, so `set_duty` passes the
    // gate's decision through with no second quantisation.
    let heater = HeaterOutput::new(transport, 1);

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

    let actuators = cc_hal_esp32::Actuators::new(
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
    Ok(actuators)
}

/// The I²C bus the ABP2 and the panel share, behind one mutex taken per
/// transaction. `None` costs the pressure sensor and the panel, not the machine.
fn shared_bus(i2c: I2cPins) -> Option<cc_hal_esp32::display_shared::SharedBus> {
    match build_shared_i2c(i2c) {
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
    }
}

/// Apply `hardware.relays.*.trigger_type` to the pump and valve drivers.
///
/// A hard-coded polarity would invert a `LOW_TRIGGER` board. In practice every
/// relay reads high-trigger, because `validate_config` refuses `LOW_TRIGGER`: such
/// a relay energises while its pin floats, before this firmware runs.
fn apply_relay_polarities(actuators: &mut cc_hal_esp32::Actuators, config: &cc_config::Config) {
    actuators.set_relay_polarities(cc_hal_esp32::actuators::RelayPolarities::from_config(
        &config.hardware.relays,
    ));
    info!(
        "actuators: relay polarity from hardware.relays.*.trigger_type — pump \
         active={:?} valve active={:?}",
        actuators.relay_polarity().pump.active,
        actuators.relay_polarity().valve.active
    );
    // A bench image must never be mistaken for a ship image.
    if cc_domain::BENCH_UNSAFE_TEMPERATURES {
        warn!(
            "SAFETY BENCH BUILD — CC_BENCH_UNSAFE_TEMPERATURES was set when this \
             image was compiled: the emergency-threshold floor and the steam and \
             brew headroom checks are compiled out. DO NOT FIT THIS TO A MACHINE. \
             It exists for operations/runbook.md §13.1."
        );
    }
}

/// The status and brew LEDs, each off as it is built.
///
/// `inverted` is the LEDs' own setting (common-anode parts), not the relays'. A
/// pin that will not configure is not fatal: the machine still makes coffee.
fn bring_up_leds(pins: LedPins, config: &cc_config::Config) -> cc_hal_esp32::leds::Leds {
    let leds_config = &config.hardware.leds;
    let leds = cc_hal_esp32::leds::Leds::new(
        led_output_pin(leds_config.status.enabled, pins.status),
        led_output_pin(leds_config.brew.enabled, pins.brew),
        leds_config.status.inverted,
        leds_config.brew.inverted,
    );
    if leds.any_configured() {
        info!(
            "leds: status=GPIO{} brew=GPIO{} driven by the control task \
             (status inverted={} brew inverted={})",
            cc_hal_esp32::leds::STATUS_PIN,
            cc_hal_esp32::leds::BREW_PIN,
            leds_config.status.inverted,
            leds_config.brew.inverted,
        );
    } else {
        info!("leds: hardware.leds.status.enabled and .brew.enabled are both false");
    }
    // GPIO1 is UART0 TXD, the provisioning console's recovery path, and a pin has
    // one owner. One warning is the whole diagnostic for the ignored setting.
    if leds_config.steam.enabled {
        warn!(
            "leds: hardware.leds.steam.enabled is set but this firmware drives no steam \
             LED — PIN_STEAMLED (GPIO1) is UART0 TX, reserved for the wifi \
             provisioning console. The steam mode indicator will not light."
        );
    }
    leds
}

/// The scale and its sampling task, or `None`. Not fatal: an absent scale is a
/// null weight in `/api/status`, not a machine that will not boot.
fn start_scale(
    pins: ScalePins,
    config: &cc_config::Config,
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
) -> Option<cc_hal_esp32::Sampler> {
    let sampler = bring_up_scale(pins, config, store).unwrap_or_else(|err| {
        warn!("scale: not started — {err}. The weight will be reported absent.");
        None
    });
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
    sampler
}

/// What the network tier hands the tasks, and what must outlive them.
struct NetworkTier {
    net: Arc<network::Network>,
    commands: Arc<cc_net_esp32::handoff::CommandQueue>,
    parameters: Arc<cc_net_esp32::handoff::ParameterHandoff>,
    handoff: network::Handoff,
    sta: Option<cc_net_esp32::Sta>,
    mqtt_configured: bool,
    /// Its `Drop` stops the httpd task.
    _http: cc_net_esp32::Web,
    /// Its `Drop` can delete the default event loop the netif depends on.
    _sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
}

/// The network stack, the radio, HTTP, telnet and the provisioning console.
///
/// The stack comes up unconditionally: an unprovisioned machine must still serve
/// `/api/status` and telnet, or nobody can find out why it is unprovisioned. The
/// radio comes up only with a stored network, its hostname set on the netif before
/// association. The MQTT client is built in the control task; here it is only
/// decided whether there is one.
fn start_network_tier(
    modem: esp_idf_hal::modem::Modem<'static>,
    uart: esp_idf_hal::uart::UART0<'static>,
    tx: esp_idf_hal::gpio::Gpio1<'static>,
    rx: esp_idf_hal::gpio::Gpio3<'static>,
    config: &cc_config::Config,
    nvs_description: &str,
) -> Result<NetworkTier, EspError> {
    let net = Arc::new(network::Network::new());
    let commands = Arc::new(cc_net_esp32::handoff::CommandQueue::new());

    let sys_loop = cc_net_esp32::wifi::init_stack()?;
    info!("netif: lwIP and the default event loop are up");
    // A failed bring-up is offline mode, not a failed boot: the HTTP server, telnet
    // and the provisioning console below are how the network gets fixed.
    let sta = if config.is_wifi_provisioned() {
        bring_up_wifi(modem, config, &sys_loop)
            .map_err(|err| warn!("wifi: bring-up failed ({err:?}) — running offline"))
            .ok()
    } else {
        info!("wifi: no SSID configured — the UART provisioning task will run");
        None
    };

    // Two producers' worth of request, one consumer: drained beside the command
    // queue at the top of the control tick.
    let parameters = Arc::new(cc_net_esp32::handoff::ParameterHandoff::new());
    let http = network::start_http(
        &net,
        config,
        nvs_description,
        Arc::clone(&commands),
        &parameters,
    )?;

    // The lowest-priority task: a stalled terminal costs a dropped line.
    if let Err(err) = cc_net_esp32::telnet::start() {
        error!("telnet: the log stream task could not be spawned: {err}");
    }

    // Always started, even with a credential stored, so a wrong network can be
    // fixed over the cable; it exits after success. The control task does the
    // write, because it holds the store.
    let handoff = network::Handoff::new();
    if config.is_wifi_provisioned() {
        info!(
            "wifi: a credential is stored — the provisioning console is still \
             available, and `wifi set` + `wifi apply` will replace it"
        );
    }
    start_provisioning(uart, tx, rx, handoff.clone());

    let mqtt_configured = cc_net_esp32::mqtt::is_configured(config);
    if !mqtt_configured {
        info!("mqtt: not configured (mqtt.enabled is false or mqtt.broker is empty)");
    }

    Ok(NetworkTier {
        net,
        commands,
        parameters,
        handoff,
        sta,
        mqtt_configured,
        _http: http,
        _sys_loop: sys_loop,
    })
}

/// The display task, with the panel brought up only when `hardware.oled.enabled`.
///
/// "Off" means the panel is never touched: no init sequence, no probe, no traffic
/// on the bus the ABP2 shares. Blanking is standby's mechanism, not this one.
fn display_task_for(
    config: &cc_config::Config,
    shared_i2c: Option<&'static cc_hal_esp32::display_shared::SharedBus>,
    frame: &Arc<slots::FrameSlot>,
    net: &network::Network,
) -> display_task::DisplayTask {
    let panel = if config.hardware.oled.enabled {
        shared_i2c.map(cc_hal_esp32::display_shared::SharedPanel::bring_up)
    } else {
        info!("display: hardware.oled.enabled is false — the panel is not brought up");
        None
    };
    display_task::DisplayTask::new(
        panel,
        display_task::template_for(config.display.template),
        Arc::clone(frame),
        Arc::clone(&net.shared),
    )
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

/// `I2C0` and its two pins, GPIO21/22, taken from `Peripherals` for the bus the
/// ABP2 and the panel share (see [`build_shared_i2c`]).
struct I2cPins {
    /// `I2C0`, the only I²C peripheral this build uses.
    peripheral: esp_idf_hal::i2c::I2C0<'static>,
    /// `PIN_I2CSDA` (GPIO21).
    sda: esp_idf_hal::gpio::Gpio21<'static>,
    /// `PIN_I2CSCL` (GPIO22).
    scl: esp_idf_hal::gpio::Gpio22<'static>,
}

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
    let serial = match cc_net_esp32::provisioning::Serial::new(uart, tx, rx) {
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
) -> Result<cc_net_esp32::Sta, EspError> {
    // The hostname goes in with the netif, before `wifi.start()`, so the DHCP
    // client identifier is fixed before the first association. See
    // `cc_net_esp32::wifi`'s module documentation.
    let mut sta = cc_net_esp32::Sta::new(modem, &config.system.hostname, sys_loop)?;
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
