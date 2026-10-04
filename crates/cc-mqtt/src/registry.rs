//! [`Registry`]: the C++'s registration, topic for topic, and [`PlanView`],
//! the flattened three-phase plan the budgeted publish pass walks.
//!
//! Moved verbatim from `cc_hal_esp32::mqtt`. `from_config` reads four booleans
//! and one `ScaleType` out of the configuration and builds nothing else; see the
//! crate root for the purity check that established it.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use cc_config::Config;
use cc_netpolicy::mqtt::{Item, Plan};

use crate::topics::{ParamTopic, Topics, BACKFLUSH_ON, CALIBRATION_ON, STEAM_MODE, TARE_ON};

/// The topics to publish, and how to retain each one.
///
/// Owned rather than borrowed, because the [`Plan`] the publish pass walks
/// borrows it while the client is mutably borrowed. Two objects, two borrows,
/// no conflict.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Retained parameter topics (`MQTTManager.cpp:410-497`).
    parameters: Vec<ParamTopic>,
    /// Non-retained polled sensors (`:500-517`).
    sensors: Vec<(String, bool)>,
    /// Retained binary sensors (`:520-548`).
    binary_sensors: Vec<(String, bool)>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            parameters: Vec::new(),
            sensors: Vec::new(),
            binary_sensors: Vec::new(),
        }
    }

    /// Add a retained parameter topic bound to a configuration key.
    pub fn add_parameter(&mut self, reading: &str, key: &'static str) {
        self.parameters.push(ParamTopic {
            topic: reading.to_string(),
            retain: true,
            key,
        });
    }

    /// Add a non-retained polled sensor.
    ///
    /// The C++ publishes these with `retain = false` (`MQTTManager.cpp:511`).
    /// Retaining a value that changes every 400 ms would fill the broker's
    /// retained store with a value that is stale by the time anyone reads it, and
    /// would cost a write per publish on a device with the flash wear budget of a
    /// coffee machine.
    pub fn add_sensor(&mut self, reading: &str) {
        self.sensors.push((reading.to_string(), false));
    }

    /// Add a retained binary sensor.
    ///
    /// `MQTTManager.cpp:543-544`: "binary state must survive broker/HA restarts
    /// — it may not change again for days, so a fresh subscriber would otherwise
    /// see unknown until the next transition."
    pub fn add_binary_sensor(&mut self, reading: &str) {
        self.binary_sensors.push((reading.to_string(), true));
    }

    /// The parameter topics, in registration order.
    #[must_use]
    pub fn parameters(&self) -> &[ParamTopic] {
        &self.parameters
    }

    /// The polled sensor topics, in registration order.
    #[must_use]
    pub fn sensors(&self) -> &[(String, bool)] {
        &self.sensors
    }

    /// The binary sensor topics, in registration order.
    #[must_use]
    pub fn binary_sensors(&self) -> &[(String, bool)] {
        &self.binary_sensors
    }

    /// How many topics in total.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parameters.len() + self.sensors.len() + self.binary_sensors.len()
    }

    /// Whether there is nothing to publish.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The configuration key an inbound reading maps to, if it is registered.
    ///
    /// `MQTTManager::assignParameter`'s `mqttVars_.find(param)`
    /// (`MQTTManager.cpp:288`). `None` is the C++'s
    /// `"MQTT topic %s not found in mapping"`: an inbound `set` for a topic this
    /// machine does not publish is refused rather than guessed at.
    #[must_use]
    pub fn resolve(&self, reading: &str) -> Option<&'static str> {
        self.parameters
            .iter()
            .find(|p| p.topic == reading)
            .map(|p| p.key)
    }

    /// Fill a registry from `config`: the C++'s registration, in its order.
    ///
    /// `SystemInitializer::registerMQTTParameters` and `registerMQTTSensors`
    /// (`src/core/SystemInitializer.cpp:687-800`), verbatim in membership and
    /// in order — 12 unconditional parameters, 14 more behind
    /// `hardware.switches.brew.enabled`, 5 or 6 behind the scale; 9
    /// unconditional sensors plus `currBrewTime`, the two weights and
    /// `pressure`; and the single binary sensor `waterTankFull`.
    ///
    /// # What this replaces
    ///
    /// Three parameters, two setpoints, `pidON`, three "sensors" and two binary
    /// sensors. Two of those are **not in the C++ and not advertised by
    /// `cc_config::discovery`** — a `brewing` and a `tankEmpty` binary sensor
    /// that no Home Assistant entity ever subscribed to, i.e. the "advertised but
    /// inert" shape reached from the publishing side. One, `weight`, is the
    /// **opposite** defect: it was published while `cc_config::discovery`
    /// advertises `currReadingWeight` and `currBrewWeight`
    /// (`MQTTManager.cpp:903-904`), which are two different topics, so the weight
    /// Home Assistant showed stayed `unknown` forever.
    ///
    /// # The topic/key pairs are the C++'s, not the discovery table's
    ///
    /// `SystemInitializer.cpp:695` registers `pidUsePonM` while
    /// `MQTTManager.cpp:869` advertises the entity as `usePonM`. The two
    /// disagree in the C++ too — the Home Assistant switch moves a topic the
    /// registry does not know, so it lands in the `sscanf` arm, finds no
    /// mapping and is dropped. Reproduced, because "fixing" it here would make
    /// the port accept a command the C++ silently ignores, and the fix belongs
    /// where the advertisement is built (`cc_config::discovery`).
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "this IS the C++'s two registration functions. Splitting the \
                  32 parameters from the 13 sensors would hide the property the \
                  function exists to provide, which is that the conditional \
                  blocks are the C++'s conditional blocks."
    )]
    pub fn from_config(topics: &Topics, config: &Config) -> Self {
        let mut registry = Self::new();
        let _ = topics;

        // ---- parameters: the twelve unconditional ones, :690-701 ----
        registry.add_parameter("pidON", "pid.enabled");
        registry.add_parameter("brewSetpoint", "brew.setpoint");
        registry.add_parameter("brewTempOffset", "brew.temp_offset");
        registry.add_parameter("steamON", STEAM_MODE);
        registry.add_parameter("steamSetpoint", "steam.setpoint");
        registry.add_parameter("pidUsePonM", "pid.use_ponm");
        registry.add_parameter("aggKp", "pid.regular.kp");
        registry.add_parameter("aggTn", "pid.regular.tn");
        registry.add_parameter("aggTv", "pid.regular.tv");
        registry.add_parameter("aggIMax", "pid.regular.i_max");
        registry.add_parameter("steamKp", "pid.steam.kp");
        registry.add_parameter("standbyModeOn", "standby.enabled");

        // ---- parameters: behind the brew switch, :705-719 ----
        if config.hardware.switches.brew.enabled {
            registry.add_parameter("aggbKp", "pid.bd.kp");
            registry.add_parameter("aggbTn", "pid.bd.tn");
            registry.add_parameter("aggbTv", "pid.bd.tv");
            registry.add_parameter("pidUseBD", "pid.bd.enabled");
            registry.add_parameter("brewPidDelay", "brew.pid_delay");
            registry.add_parameter("targetBrewTime", "brew.by_time.target_time");
            registry.add_parameter("preinfusion", "brew.pre_infusion.time");
            registry.add_parameter("preinfusionPause", "brew.pre_infusion.pause");
            registry.add_parameter("backflushOn", BACKFLUSH_ON);
            registry.add_parameter("backflushCycles", "backflush.cycles");
            registry.add_parameter("backflushFillTime", "backflush.fill_time");
            registry.add_parameter("backflushFlushTime", "backflush.flush_time");
            registry.add_parameter(
                "backflushReminderEnabled",
                "maintenance.backflush_reminder.enabled",
            );
            registry.add_parameter(
                "backflushReminderThreshold",
                "maintenance.backflush_reminder.threshold",
            );
        }

        // ---- parameters: behind the scale, :722-734 ----
        if config.hardware.sensors.scale.enabled {
            registry.add_parameter("targetBrewWeight", "brew.by_weight.target_weight");
            registry.add_parameter("scaleCalibration", "hardware.sensors.scale.calibration");
            if config.hardware.sensors.scale.r#type == cc_domain::hardware::ScaleType::Hx711Dual {
                registry.add_parameter("scale2Calibration", "hardware.sensors.scale.calibration2");
            }
            registry.add_parameter("scaleKnownWeight", "hardware.sensors.scale.known_weight");
            registry.add_parameter("scaleTareOn", TARE_ON);
            registry.add_parameter("scaleCalibrationOn", CALIBRATION_ON);
        }

        // ---- sensors: the nine unconditional ones, :738-766 ----
        registry.add_sensor("temperature");
        registry.add_sensor("heaterPower");
        registry.add_sensor("standbyModeTimeRemaining");
        registry.add_sensor("shotsSinceBackflush");
        registry.add_sensor("backflushReminderDue");
        registry.add_sensor("currentKp");
        registry.add_sensor("currentKi");
        registry.add_sensor("currentKd");
        registry.add_sensor("machineState");

        // ---- sensors: behind the brew switch, :770-775 ----
        if config.hardware.switches.brew.enabled {
            registry.add_sensor("currBrewTime");
        }

        // ---- sensors: behind the scale, :778-784 ----
        if config.hardware.sensors.scale.enabled {
            registry.add_sensor("currReadingWeight");
            registry.add_sensor("currBrewWeight");
        }

        // ---- sensors: behind the pressure probe, :787-790 ----
        if config.hardware.sensors.pressure.enabled {
            registry.add_sensor("pressure");
        }

        // ---- binary sensors: behind the water tank, :794-798 ----
        if config.hardware.sensors.watertank.enabled {
            registry.add_binary_sensor("waterTankFull");
        }

        registry
    }
}

/// A borrowed view of a [`Registry`], plus its own item storage.
///
/// A `Plan` borrows three slices, so building one out of a `Vec<ParamTopic>` and
/// two `Vec<(String, bool)>` needs the items to live somewhere for the duration.
/// This owns them for the duration and hands out a [`Plan`] that borrows from
/// itself.
pub struct PlanView<'a> {
    /// `pub(crate)` because `tests::a_registry_never_names_a_credential` reads
    /// the flattened items directly, to assert that no topic in the view carries
    /// the broker password. It was a private field in `cc-hal-esp32` and the
    /// test reached it as a sibling module's private; across a crate boundary
    /// `pub(crate)` is the same reach.
    pub(crate) items: Vec<Item<'a>>,
    /// Where each phase **ends** in `items`, as absolute offsets:
    /// `[parameters_end, sensors_end, binary_sensors_end]`.
    ///
    /// Offsets, not lengths. The three groups are stored contiguously and a
    /// [`Plan`] is three contiguous slices of that one `Vec`, so `plan` needs
    /// offsets. Storing lengths here was a device bug: every plan past the first
    /// group was a wrong slice, and for any registry with a binary sensor the
    /// third slice was out of range and **panicked**, taking the MQTT task and
    /// the chip with it the first time anything was published.
    ends: [usize; 3],
}

impl<'a> PlanView<'a> {
    /// Build the view.
    #[must_use]
    pub fn of(registry: &'a Registry) -> Self {
        let mut items: Vec<Item<'a>> = Vec::with_capacity(registry.len());
        for param in &registry.parameters {
            items.push(Item {
                topic: param.topic.as_str(),
                retain: param.retain,
            });
        }
        let end_of_parameters = items.len();
        for (topic, retain) in &registry.sensors {
            items.push(Item {
                topic: topic.as_str(),
                retain: *retain,
            });
        }
        let end_of_sensors = items.len();
        for (topic, retain) in &registry.binary_sensors {
            items.push(Item {
                topic: topic.as_str(),
                retain: *retain,
            });
        }
        let ends = [end_of_parameters, end_of_sensors, items.len()];
        Self { items, ends }
    }

    /// The plan, borrowing this view's items.
    ///
    /// Rebuilt per phase because the three phases are contiguous slices of one
    /// `Vec` and the offsets are what the constructor recorded.
    #[must_use]
    pub fn plan(&'a self) -> Plan<'a> {
        let [p_end, s_end, _b_end] = self.ends;
        Plan {
            parameters: &self.items[..p_end],
            sensors: &self.items[p_end..s_end],
            binary_sensors: &self.items[s_end..],
        }
    }

    /// The number of items, for a log line.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the view is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
