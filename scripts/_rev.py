p = "crates/cc-hal-esp32/src/task.rs"
s = open(p).read()

# Drop the leaking `owned()` experiment and publish the rendered body instead:
# one String, one allocation, no lifetime problem and no per-field copy.
start = s.index("    /// The same value, owned.")
end = s.index("    #[must_use]\n    pub fn kind(&self)", start)
s = s[:start] + s[end:]

s = s.replace(
    """    pub fn publish_live(&self, values: alloc::vec::Vec<cc_config::LiveValue<'static>>) {
        if let Ok(mut slot) = self.live.lock() {
            *slot = Some(values);
        }
    }

    /// The last published values, if the control task has published any.
    #[must_use]
    pub fn live(&self) -> Option<alloc::vec::Vec<cc_config::LiveValue<'static>>> {
        self.live.lock().ok().and_then(|slot| slot.clone())
    }""",
    """    pub fn publish_live(&self, body: alloc::string::String) {
        if let Ok(mut slot) = self.live.lock() {
            *slot = Some(body);
        }
    }

    /// The last published body, if the control task has published one.
    #[must_use]
    pub fn live(&self) -> Option<alloc::string::String> {
        self.live.lock().ok().and_then(|slot| slot.clone())
    }""",
)
s = s.replace(
    "    live: alloc::sync::Arc<Mutex<Option<alloc::vec::Vec<cc_config::LiveValue<'static>>>>>,",
    "    live: alloc::sync::Arc<Mutex<Option<alloc::string::String>>>,",
)

# The doc comment now describes a rendered body rather than a value list.
s = s.replace(
    """    /// So the control task publishes the values it is actually running with,
    /// here, and the GET renders from those. It is a copy rather than a
    /// reference because `Config` is the control task's and crossing a task
    /// boundary with `&mut` to it is neither possible nor wanted.""",
    """    /// So the control task publishes the **rendered body** it would have
    /// served, and the GET sends that. One `String` per heartbeat rather than
    /// 98 copied values, and no lifetime to get wrong: `LiveValue::Text` borrows
    /// from the `Config`, so a value list could not have crossed this boundary
    /// without either leaking or inventing a second owned enum.""",
)
open(p, "w").write(s)
print("ok")
