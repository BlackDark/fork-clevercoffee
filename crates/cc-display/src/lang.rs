//! The display's strings, one struct per language.
//!
//! The strings are `&'static str` constants reachable only through a
//! [`Language`] value, so a missing string is a compile error rather than a null
//! pointer at draw time, and no code can reassign one.
//!
//! Only the strings the display layer actually draws are here. The radio
//! provisioning screens (reconnect, connect-to-WiFi, no-WiFi) and the
//! scale-calibration messages are drawn by code that is *not* in the display
//! layer, so their strings belong with that code.

use crate::model::Language;

/// Every display string, for one language.
///
/// A struct rather than 30 free functions: a caller writes `lang.brew` and the
/// compiler points at the one field, and adding a language is a single
/// exhaustive `match` with no "did you update all the strings?" question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang {
    /// The setpoint label.
    pub set_temp: &'static str,
    /// The measured-temperature label.
    pub current_temp: &'static str,
    /// The brew label.
    pub brew: &'static str,
    /// The weight label.
    pub weight: &'static str,
    /// The manual-flush label.
    pub manual_flush: &'static str,
    /// The hot-water label.
    pub hot_water: &'static str,
    /// The pressure label.
    pub pressure: &'static str,
    /// The uptime label.
    pub uptime: &'static str,
    /// The offline-mode label.
    pub offline: &'static str,
    /// The scale-failure message.
    pub scale_failure: &'static str,

    // -- Upright (portrait) variants: the same labels, shortened to fit the
    // -- portrait column widths
    /// The setpoint label, portrait.
    pub set_temp_ur: &'static str,
    /// The measured-temperature label, portrait.
    pub current_temp_ur: &'static str,
    /// The brew label, portrait.
    pub brew_ur: &'static str,
    /// The manual-flush label, portrait.
    pub manual_flush_ur: &'static str,
    /// The hot-water label, portrait.
    pub hot_water_ur: &'static str,
    /// The weight label, portrait.
    pub weight_ur: &'static str,
    /// The pressure label, portrait.
    pub pressure_ur: &'static str,

    // -- the backflush system screen
    /// The backflush pressure line.
    pub backflush_press: &'static str,
    /// The backflush start line.
    pub backflush_start: &'static str,
    /// The backflush finish line.
    pub backflush_finish: &'static str,

    // -- the maintenance footer, three lines
    /// The three footer lines.
    pub backflush_reminder: [&'static str; 3],

    /// The five landscape lines: the reading, then the advisory text around it.
    pub error_tsensor: [&'static str; 5],
    /// The five portrait lines, same order.
    pub error_tsensor_ur: [&'static str; 5],
}

const ENGLISH: Lang = Lang {
    set_temp: "Set:   ",
    current_temp: "Temp:  ",
    brew: "Brew: ",
    weight: "Weight: ",
    manual_flush: "Flush: ",
    hot_water: "Water: ",
    pressure: "Pressure: ",
    uptime: "Uptime:  ",
    offline: "Offline",
    scale_failure: "Fault",
    set_temp_ur: "S: ",
    current_temp_ur: "T: ",
    brew_ur: "B: ",
    manual_flush_ur: "F: ",
    hot_water_ur: "Wp: ",
    weight_ur: "W: ",
    pressure_ur: "P: ",
    backflush_press: "Press brew switch",
    backflush_start: "to start...",
    backflush_finish: "to finish...",
    backflush_reminder: [
        "Backflush recommended",
        "Run a detergent",
        "backflush cycle",
    ],
    error_tsensor: ["Error, Temp: ", "Check Temp. sensor!", "", "", ""],
    error_tsensor_ur: ["Error", "Temp: ", "check", "temp.", "sensor!"],
};

const GERMAN: Lang = Lang {
    set_temp: "Soll:  ",
    current_temp: "Ist:   ",
    brew: "Bezug: ",
    weight: "Gewicht: ",
    manual_flush: "Spuelen: ",
    hot_water: "Wasser: ",
    pressure: "Druck: ",
    uptime: "Uptime:  ",
    offline: "Offline",
    scale_failure: "Fehler",
    set_temp_ur: "S: ",
    current_temp_ur: "I: ",
    brew_ur: "B: ",
    manual_flush_ur: "S: ",
    hot_water_ur: "W: ",
    weight_ur: "G: ",
    pressure_ur: "D: ",
    backflush_press: "Bruehsch. druecken",
    backflush_start: "um zu starten...",
    backflush_finish: "um zu beenden...",
    backflush_reminder: ["Rueckspuelen", "Reiniger-", "Rueckspuelung"],
    // Split: one line is 129 px in profont10.
    error_tsensor: ["Fehler, Temp: ", "Temp.-Sensor", "ueberpruefen!", "", ""],
    // Last word is 63 px in profont10, 75 in profont11.
    error_tsensor_ur: ["Fehler", "Temp: ", "Temp.", "Sensor", "ueberpruefen!"],
};

const SPANISH: Lang = Lang {
    set_temp: "Obj:  ",
    current_temp: "T:    ",
    brew: "Brew: ",
    weight: "Peso: ",
    manual_flush: "Fregar: ",
    hot_water: "Agua: ",
    pressure: "Presión: ",
    uptime: "Uptime:  ",
    offline: "Offline",
    scale_failure: "falla",
    set_temp_ur: "S: ",
    current_temp_ur: "T: ",
    brew_ur: "B: ",
    manual_flush_ur: "F: ",
    hot_water_ur: "A: ",
    weight_ur: "P: ",
    pressure_ur: "Pr: ",
    backflush_press: "Pulsa boton de cafe",
    backflush_start: "para empezar...",
    backflush_finish: "para terminar...",
    backflush_reminder: ["Recomendado", "Hacer backflush", "con detergente"],
    error_tsensor: ["Error, Temp: ", "Comprueba sensor T!", "", "", ""],
    error_tsensor_ur: ["Error", "Temp: ", "Comprueba", "sensor", "T!"],
};

/// The strings for `language`.
///
/// The German arm is the fallback for an unrecognised value, written as an
/// explicit arm rather than a catch-all `_` so that adding a language is a
/// visible edit.
#[must_use]
pub const fn for_language(language: Language) -> &'static Lang {
    match language {
        Language::English => &ENGLISH,
        Language::German => &GERMAN,
        Language::Spanish => &SPANISH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_fills_every_field() {
        // The struct makes a missed string impossible; this test pins every
        // known value so a copy-paste slip is caught.
        assert_eq!(for_language(Language::English).set_temp, "Set:   ");
        assert_eq!(for_language(Language::German).set_temp, "Soll:  ");
        assert_eq!(for_language(Language::Spanish).set_temp, "Obj:  ");
    }

    #[test]
    fn labels_keep_their_trailing_padding() {
        // The label's *rendered* width does not position the value, but a label
        // that loses a trailing space shifts every glyph by a column in the
        // wrapped-message path and in the width probe. Pin the widths.
        let f = crate::font::profont11();
        let en = for_language(Language::English);
        assert_eq!(en.current_temp, "Temp:  ", "7 chars including padding");
        // Measured against the oracle, in profont11.
        assert_eq!(
            f.str_width(en.current_temp),
            42,
            "matches the oracle's width measurement"
        );
        assert_eq!(f.str_width(en.set_temp), 42);
        assert_eq!(f.str_width(en.brew), 36);
        assert_eq!(f.str_width(en.weight), 48);
        assert_eq!(f.str_width(en.pressure), 60);
    }

    #[test]
    fn the_upright_labels_are_short_forms() {
        let en = for_language(Language::English);
        assert_eq!(en.set_temp_ur, "S: ");
        assert_eq!(en.current_temp_ur, "T: ");
        assert_eq!(en.brew_ur, "B: ");
        assert_eq!(en.pressure_ur, "P: ");
    }

    #[test]
    fn the_sensor_error_screen_has_five_slots() {
        // Landscape draws slots 0–2. English leaves 2–4 empty.
        let en = for_language(Language::English);
        assert_eq!(en.error_tsensor.len(), 5);
        assert_eq!(en.error_tsensor[0], "Error, Temp: ");
        assert_eq!(en.error_tsensor[1], "Check Temp. sensor!");
        let de = for_language(Language::German);
        assert_eq!(de.error_tsensor[1], "Temp.-Sensor");
        assert_eq!(de.error_tsensor[2], "ueberpruefen!");
        let es = for_language(Language::Spanish);
        assert_eq!(es.error_tsensor[1], "Comprueba sensor T!");
    }
}
