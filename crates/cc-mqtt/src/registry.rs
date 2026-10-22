//! Publish membership and retain flags derived from [`Config`].
//!
//! [`PlanView`] flattens the registry into the budgeted pass's three phases.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use cc_config::Config;
use cc_netpolicy::mqtt::{Item, Plan};

use crate::topics::{ParamTopic, Topics, BACKFLUSH_ON, CALIBRATION_ON, STEAM_MODE, TARE_ON};

/// The topics to publish, and how each is retained.
///
/// Owned, not borrowed: the [`Plan`] the pass walks borrows this while the client is
/// mutably borrowed, and two objects make the two borrows disjoint.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Retained parameter topics, in registration order.
    parameters: Vec<ParamTopic>,
    /// Non-retained polled sensors, in registration order.
    sensors: Vec<(String, bool)>,
    /// Retained binary sensors, in registration order.
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
    /// **Not** retained. A value that changes every 400 ms would fill the broker's
    /// retained store with something stale, on a device with the flash wear budget
    /// of a coffee machine. A subscriber wanting the current value gets it from the
    /// retained parameters; polled topics exist to carry change.
    pub fn add_sensor(&mut self, reading: &str) {
        self.sensors.push((reading.to_string(), false));
    }

    /// Add a retained binary sensor.
    ///
    /// **Retained**, unlike a polled sensor: binary state may not change again for
    /// days, so without the retained copy a new subscriber sees `unknown` until the
    /// next transition, which can be never.
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

    /// The configuration key an inbound reading maps to, or `None` if not
    /// registered.
    ///
    /// **Refused, not guessed at.** An unregistered name must not resolve to a
    /// neighbouring parameter.
    #[must_use]
    pub fn resolve(&self, reading: &str) -> Option<&'static str> {
        self.parameters
            .iter()
            .find(|p| p.topic == reading)
            .map(|p| p.key)
    }

    /// Fill a registry from `config`, in publication order.
    ///
    /// Membership is gated on four configuration booleans and the scale type, so
    /// machines with different hardware publish different topic sets. Unconditional
    /// entries come first in each of the three lists.
    ///
    /// # A registered topic is not the same as an advertised entity
    ///
    /// [`cc_config::discovery`] builds the entities; this builds what is published.
    /// They disagree at exactly one name: discovery advertises `usePonM`, the
    /// registry registers `pidUsePonM`, so that switch's writes are refused.
    ///
    /// The safe direction, deliberately: a switch that looks wired and does nothing
    /// is worse than one visibly inert. Renaming the entry here would also change a
    /// wire format, so both sides must move together — and only in `cc_config::discovery`.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "one flat table per phase. Splitting the conditional groups \
                  into helpers would scatter a single ordered list, and the \
                  order is part of what this provides."
    )]
    pub fn from_config(topics: &Topics, config: &Config) -> Self {
        let mut registry = Self::new();
        let _ = topics;

        // ---- parameters: the twelve unconditional ones ----
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

        // ---- parameters: behind the brew switch ----
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

        // ---- parameters: behind the scale ----
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

        // ---- sensors: the nine unconditional ones ----
        registry.add_sensor("temperature");
        registry.add_sensor("heaterPower");
        registry.add_sensor("standbyModeTimeRemaining");
        registry.add_sensor("shotsSinceBackflush");
        registry.add_sensor("backflushReminderDue");
        registry.add_sensor("currentKp");
        registry.add_sensor("currentKi");
        registry.add_sensor("currentKd");
        registry.add_sensor("machineState");

        // ---- sensors: behind the brew switch ----
        if config.hardware.switches.brew.enabled {
            registry.add_sensor("currBrewTime");
        }

        // ---- sensors: behind the scale ----
        if config.hardware.sensors.scale.enabled {
            registry.add_sensor("currReadingWeight");
            registry.add_sensor("currBrewWeight");
        }

        // ---- sensors: behind the pressure probe ----
        if config.hardware.sensors.pressure.enabled {
            registry.add_sensor("pressure");
        }

        // ---- binary sensors: behind the water tank ----
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
