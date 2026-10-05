p = "crates/cc-config/src/json.rs"
s = open(p).read()

anchor = """impl LiveValue<'_> {"""
new = """impl LiveValue<'_> {
    /// The same value, owned.
    ///
    /// `Text` borrows from a `&Config`, so it cannot cross a task boundary or
    /// outlive the call that produced it. A published snapshot — the one
    /// `GET /api/parameters` reads — needs owned strings.
    ///
    /// This is the only place a credential is copied out of the `Config`, and
    /// it is worth being explicit about why: the copy is what lets the HTTP
    /// layer answer without borrowing the control task's configuration. The
    /// value never leaves flash unencrypted either way — R2-06 recorded that
    /// NVS encryption was declined for parity with the C++ — so this copy
    /// changes nothing about what is on the device, only about who can read it
    /// in RAM while the machine runs.
    #[must_use]
    pub fn owned(&self) -> LiveValue<'static> {
        match self {
            Self::Bool(v) => LiveValue::Bool(*v),
            Self::Int(v) => LiveValue::Int(*v),
            Self::Float(v) => LiveValue::Float(*v),
            Self::Text(v) => LiveValue::Text(alloc::boxed::Box::leak(
                alloc::string::String::from(*v).into_boxed_str(),
            )),
            Self::Enum(v) => LiveValue::Enum(*v),
        }
    }
"""
assert anchor in s
s = s.replace(anchor, new, 1)
open(p, "w").write(s)
print("ok")
