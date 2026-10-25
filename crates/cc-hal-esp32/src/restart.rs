//! Drain console output before resetting the chip.
//!
//! `esp_restart()` drops buffered UART bytes. [`drain_console`] flushes software and
//! hardware queues; [`restart_now`] is the shared reboot path. The end-to-end
//! device test proves the drain using its own logger, not the firmware's fanout.
//! This flushes console bytes; it makes no actuator-shutdown guarantee.

use esp_idf_svc::sys::{
    fflush, uart_is_driver_installed, uart_port_t_UART_NUM_0, uart_wait_tx_done, TickType_t,
};

/// How long [`drain_console`] waits for the console's TX path, in `FreeRTOS` ticks.
///
/// ESP-IDF's `CONFIG_FREERTOS_HZ` for this target is 1000, so this is one
/// second. A bound rather than `portMAX_DELAY`: a caller that has wedged the
/// UART should still get its reboot, and the alternative to a hang on a machine
/// with the pump running is worse than a truncated line.
const CONSOLE_DRAIN_TICKS: TickType_t = 1_000;

/// Push everything this process has written to the console out onto the wire.
///
/// Two steps, both needed:
///
/// 1. `fflush(NULL)` drains libc's `FILE` buffers for **all** streams. ESP-IDF's
///    VFS console is a `FILE` with a `fstatcookie` driver, and anything written
///    through libc's `printf` family can still be sitting in it.
/// 2. `uart_wait_tx_done(UART0)` waits for the driver's ring buffer **and** the
///    UART's shift register to empty. Without this the last few bytes are in a
///    register that the reset stops clocking.
///
/// Step 2 is conditional on a driver being installed, and in this firmware it
/// is **not**: the console is the ROM/VFS one (`esp_vfs_dev_uart`, `CONFIG_
/// ESP_CONSOLE_UART`), and `uart_wait_tx_done` against a port with no driver
/// returns `ESP_ERR_INVALID_STATE` and logs
/// `uart: uart_wait_tx_done(): uart driver error` on every single reboot.
/// Verified on the device. So the guard is not defensive politeness: calling it
/// unconditionally makes every restart in the firmware print a spurious error
/// immediately before it goes down, which is precisely the kind of last-line
/// noise that hides a real one.
///
/// Ignores the return of both: there is nothing useful a caller can do about a
/// failed flush that is about to be reset anyway, and reporting an error from a
/// function whose whole purpose is to run immediately before the reset is noise.
#[allow(
    unsafe_code,
    reason = "Three plain C calls with no pointer arguments and no return value \
              worth checking. `fflush(NULL)` is the ISO C spelling for 'flush \
              every output stream'; `uart_is_driver_installed` reads a bool out \
              of the port; `uart_wait_tx_done(UART0, ticks)` is declared in \
              `driver/uart.h` and blocks. None allocates, takes a lock, or \
              retains anything. Same reasoning and same exception as \
              `cc_hal_esp32::time` and `::zacwire`."
)]
pub fn drain_console() {
    // SAFETY: `fflush(NULL)` is defined by ISO C to flush all open output
    // streams; a null `FILE *` is the specified spelling, not a null
    // dereference, and the function cannot retain the pointer.
    unsafe {
        fflush(core::ptr::null_mut());
    }
    // SAFETY: both take a `uart_port_t` by value, allocate nothing, retain
    // nothing, and are safe to call from a task. `UART_NUM_0` is 0, a valid
    // port on this chip.
    unsafe {
        if uart_is_driver_installed(uart_port_t_UART_NUM_0) {
            uart_wait_tx_done(uart_port_t_UART_NUM_0, CONSOLE_DRAIN_TICKS);
        }
    }
}

/// Reset the chip, after draining the console.
///
/// `-> !`, because `esp_restart()` does not return. The `!` is derived from the
/// FFI rather than asserted: the call is followed by an unreachable
/// `unreachable!()` so the compiler can see the return type through the
/// `unsafe` block.
///
/// Every reboot path goes through here. Calling `esp_restart()` directly is the
/// bug this function exists to prevent.
#[allow(
    unsafe_code,
    reason = "`esp_restart()` is `void esp_restart(void)`: no arguments, no \
              return, no allocation, no lock, callable from any task, and \
              documented not to return. There is no safe wrapper for it in \
              esp-idf-hal 0.47. The alternative the caller would otherwise write \
              is a loop that never returns, which leaves the relays in whatever \
              state the last tick left them rather than dropping them on reset."
)]
pub fn restart_now() -> ! {
    drain_console();
    // SAFETY: `esp_restart()` is declared `void esp_restart(void)` in
    // `esp_system.h`. It takes no pointer, allocates nothing, takes no lock,
    // allocates no memory, and is documented as callable from any task. It does
    // not return.
    unsafe {
        esp_idf_svc::sys::esp_restart();
    }
}
