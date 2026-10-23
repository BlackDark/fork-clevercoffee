//! Task spawning at explicit `FreeRTOS` priorities, the priority table, and the
//! delay primitive.

use esp_idf_hal::task::thread::ThreadSpawnConfiguration;

/// Sleep the calling task.
pub fn delay_ms(ms: u32) {
    esp_idf_hal::delay::FreeRtos::delay_ms(ms);
}

/// Spawn a task at an explicit `FreeRTOS` priority.
///
/// # Why a `std::thread::Builder` alone is not enough
///
/// `std::thread::Builder` on this target is `esp_pthread`
/// (`components/pthread/port/linux/pthread.c`), which reads its priority from
/// the **process-wide** `esp_pthread` default,
/// `CONFIG_PTHREAD_TASK_PRIO_DEFAULT`. That default is 5 in this build, and
/// control is
/// 5, display is 3 and provisioning is 4 — so the control task got the right
/// number by coincidence and the other two did not. Worse, lwIP's own `tcpip`
/// task runs at **18**, so a bare spawn left network work preempting the control
/// loop, which is the inversion ADR-0002 records and which the table exists to
/// remove. Naming the
/// priorities and never applying them is worse than not naming them: the log
/// claims a relationship nothing establishes.
///
/// [`ThreadSpawnConfiguration`] is the only priority API in `esp-idf-hal` 0.47
/// (`task.rs:353-405`) and it is **global**, so the default is saved, the
/// task's configuration is installed, the thread is spawned, and the default is
/// put back. Leaving the task's priority installed would silently raise
/// everything spawned afterwards — including the httpd task, which
/// `esp-idf-svc` starts from a `std::thread` of its own.
///
/// This is the pattern [`crate::scale::Sampler::start`] already uses
/// (`scale.rs:640-662`); it is here rather than inlined three times because
/// three tasks need it.
///
/// # Errors
///
/// [`std::io::Error`] from the thread spawn, and [`EspError`] if the global
/// configuration could not be installed or — the case that matters — restored.
/// A failure to restore leaves every later task at this task's priority, so it
/// is reported rather than dropped.
pub fn spawn_with_prio<F, T>(
    name: &'static core::ffi::CStr,
    stack_size: usize,
    priority: u8,
    body: F,
) -> std::io::Result<std::thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let mut config = ThreadSpawnConfiguration::get().unwrap_or_default();
    config.stack_size = stack_size;
    config.priority = priority;
    // `&'static CStr` rather than `&str`: this is the field's own type, and
    // converting would mean either an allocation whose `CString` cannot outlive
    // the `'static` borrow `config.name` needs, or
    // `CStr::from_bytes_with_nul_unchecked`, which is `unsafe` and this
    // workspace denies `unsafe_code` (`Cargo.toml:125`). A `c"control"` literal
    // at the call site is cheaper than either.
    config.name = Some(name);

    // `set` panics on a priority outside `1..24`. Every caller passes a
    // compile-time constant, so this cannot fire today; it is left to fire
    // because a panic at boot is the right outcome for a priority the
    // scheduler would reject anyway.
    config.set().map_err(std::io::Error::other)?;

    let spawned = std::thread::Builder::new()
        .name(name.to_string_lossy().into_owned())
        .stack_size(stack_size)
        .spawn(body);

    // Restore the process default whatever happened — including on the error
    // path, which is the case that matters.
    if let Some(previous) = ThreadSpawnConfiguration::get() {
        previous.set().map_err(std::io::Error::other)?;
    }

    spawned
}

/// The control task's `FreeRTOS` priority.
///
/// 5: the control loop is the
/// highest-priority non-ISR
/// task in the firmware, so a network or display task cannot preempt a
/// decision about the heater.
///
/// **Still below lwIP's `tcpip` task at 18**, which is IDF's own default and
/// is not this firmware's to change. What this constant fixes is the
/// *documented* relationship between *this firmware's* tasks; the
/// control-vs-lwIP ordering is recorded in
/// [`crate::scale::SAMPLER_PRIO`]'s doc, which is where it is measured.
pub const CONTROL_PRIO: u8 = 5;

/// The display task's priority, 3.
///
/// Below [`CONTROL_PRIO`] on purpose: a frame is ten milliseconds of I²C bus
/// time, and the tick that must not be delayed is the one that decides whether
/// the heater stays on.
pub const DISPLAY_PRIO: u8 = 3;

/// The provisioning task's priority, 4.
///
/// Between display and control: the UART console is interactive, so it should
/// answer while a frame is going out, but it is still a network-tier task and
/// must not delay the tick.
pub const PROVISION_PRIO: u8 = 4;

/// The telnet log stream's priority, 2.
///
/// **The lowest priority in the firmware, and that is the requirement rather
/// than an accident of numbering.** The stream's only job is to move bytes to a
/// terminal; a client that has stopped reading is a condition to *drop* lines
/// for, never to spend the machine's time on. So it sits below the display
/// task's 3 and below provisioning's 4, and the ring
/// ([`cc_net::web::telnet::Ring`]) is what keeps a stalled client off the control
/// tick entirely: the producer claims a slot, copies, and returns, whatever the
/// socket is doing.
///
/// Below [`CONTROL_PRIO`] on its own would have been enough for the tick, but
/// the display task's doc names a ten-millisecond I²C frame as the thing that
/// must not wait, and a `lwip_send` into a full TCP window is a longer wait
/// than that.
pub const TELNET_PRIO: u8 = 2;

/// The priorities are a relationship, and a relationship expressed only in a
/// table is not checked.
///
/// A display task at 5 would compile, run, and reintroduce exactly the tick
/// overrun that moving the display out of the control loop fixed. Asserted
/// at compile time for the reason [`crate::scale::SAMPLER_PRIO`] asserts its
/// relationship the same way.
const _: () = {
    assert!(
        CONTROL_PRIO > DISPLAY_PRIO,
        "a display frame is 10 ms of I²C bus time and must not preempt \
         the tick that decides whether the heater stays on"
    );
    assert!(
        CONTROL_PRIO > PROVISION_PRIO,
        "provisioning is a network-tier task and must not preempt the \
         control loop"
    );
    assert!(
        PROVISION_PRIO > DISPLAY_PRIO,
        "an interactive UART console should answer while a frame is \
         being written to the panel"
    );
    assert!(
        CONTROL_PRIO > TELNET_PRIO,
        "the telnet stream must never preempt the tick that decides whether \
         the heater stays on"
    );
    assert!(
        DISPLAY_PRIO > TELNET_PRIO,
        "a `lwip_send` into a full TCP window blocks for longer than a 10 ms \
         I²C frame, and the log stream is the least urgent thing the firmware \
         does"
    );
};
