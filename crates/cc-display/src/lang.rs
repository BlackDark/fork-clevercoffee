//! Port of `include/clevercoffee/display/languages.h`.
//!
//! The C++ has a mutable set of `static const char*` globals initialised by
//! `initLangStrings()` from a config switch, which means every string is a
//! global that any code can reassign. That is not portable and not testable, so
//! the port is a `match` on [`Language`]: the strings become `&'static str`
//! constants reachable only through the language, and a missing one is a
//! compile error rather than a null pointer.
//!
//! Only the strings the display layer actually draws are here. The radio
//! provisioning screens (`langstring_wifirecon`, `langstring_connectwifi1`,
//! `langstring_nowifi`) and the scale-calibration messages are drawn by code
//! that is *not* in `display/`, and are R2-11's business.

use crate::model::Language;

/// Every display string, for one language.
///
/// A struct rather than 30 free functions: a caller writes `lang.brew` and the
/// compiler points at the one field, and adding a language is a single
/// exhaustive `match` with no "did you update all the strings?" question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang {
    /// `langstring_set_temp` — the setpoint label.
    pub set_temp: &'static str,
    /// `langstring_current_temp` — the measured label.
    pub current_temp: &'static str,
    /// `langstring_brew`
    pub brew: &'static str,
    /// `langstring_weight`
    pub weight: &'static str,
    /// `langstring_manual_flush`
    pub manual_flush: &'static str,
    /// `langstring_hot_water`
    pub hot_water: &'static str,
    /// `langstring_pressure`
    pub pressure: &'static str,
    /// `langstring_uptime`
    pub uptime: &'static str,
    /// `langstring_offlinemode`
    pub offline: &'static str,
    /// `langstring_scale_Failure`
    pub scale_failure: &'static str,

    // -- Upright (portrait) variants, which are short
    /// `langstring_set_temp_ur`
    pub set_temp_ur: &'static str,
    /// `langstring_current_temp_ur`
    pub current_temp_ur: &'static str,
    /// `langstring_brew_ur`
    pub brew_ur: &'static str,
    /// `langstring_manual_flush_ur`
    pub manual_flush_ur: &'static str,
    /// `langstring_hot_water_ur`
    pub hot_water_ur: &'static str,
    /// `langstring_weight_ur`
    pub weight_ur: &'static str,
    /// `langstring_pressure_ur`
    pub pressure_ur: &'static str,

    // -- the backflush system screen
    /// `langstring_backflush_press`
    pub backflush_press: &'static str,
    /// `langstring_backflush_start`
    pub backflush_start: &'static str,
    /// `langstring_backflush_finish`
    pub backflush_finish: &'static str,

    // -- the maintenance footer, three lines
    /// `langstring_backflush_reminder[0..3]`
    pub backflush_reminder: [&'static str; 3],

    /// Landscape: `[0]`, the reading, `[1]`, `[2]`. `languages.h`.
    pub error_tsensor: [&'static str; 5],
    /// Portrait lines. `languages.h`.
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
    // Split: one line is 129 px in profont10 (`languages.h`).
    error_tsensor: ["Fehler, Temp: ", "Temp.-Sensor", "ueberpruefen!", "", ""],
    // Last word is 63 px in profont10, 75 in profont11 (`languages.h:155-159`).
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
/// The C++ `initLangStrings()` has an `else` arm for German, so an
/// unrecognised value is German. That fallback is preserved here as an
/// explicit arm rather than a catch-all `_`, because a new language should be
/// a visible edit.
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
        // The C++ initialises every global in each branch. A missed
        // initialisation there is a null pointer at draw time; here the struct
        // makes it impossible, and this test pins the three known values so a
        // copy-paste slip is caught.
        assert_eq!(for_language(Language::English).set_temp, "Set:   ");
        assert_eq!(for_language(Language::German).set_temp, "Soll:  ");
        assert_eq!(for_language(Language::Spanish).set_temp, "Obj:  ");
    }

    #[test]
    fn labels_keep_their_trailing_padding() {
        // `displayTemperatureInfo` does `setCursor(currentValueX, y)` with a
        // hard-coded x, so the label's *rendered* width is not what positions
        // the value -- but a label that loses a trailing space still shifts
        // every glyph by a column in the wrapped-message path and in the
        // `getStrWidth` probe. Pin the widths.
        let f = crate::font::profont11();
        let en = for_language(Language::English);
        assert_eq!(en.current_temp, "Temp:  ", "7 chars including padding");
        // Measured against the oracle (`getStrWidth` in profont11).
        assert_eq!(
            f.str_width(en.current_temp),
            42,
            "matches the C++ getStrWidth"
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
