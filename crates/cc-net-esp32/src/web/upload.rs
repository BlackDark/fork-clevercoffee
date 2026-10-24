//! `POST /api/ota/{firmware,filesystem}`: stream the body into flash through
//! [`crate::ota`], then hand the restart to the control task.

#![allow(
    clippy::wildcard_imports,
    reason = "split out of `web` for size: it shares the parent's imports and \
              private helpers"
)]

use super::*;

/// Why a streamed upload stopped, and what the client is told.
///
/// A struct rather than a `Result<(), StatusMessage>` because the three cases
/// carry **three different HTTP statuses** — a malformed envelope is a `400`, a
/// flash failure is a `500`, and a mid-stream write failure is a `500` with a
/// different word. Collapsing them to one error would force the caller to
/// re-derive which.
struct OtaFailure {
    /// 400 for a malformed body, 500 for a device-side failure.
    status: u16,
    /// The `StatusMessage` for `GET /api/ota/status`.
    message: cc_net::web::ota::StatusMessage,
    /// The body `result.message`, which the UI shows verbatim.
    text: &'static str,
}

/// Read the socket to its end, stripping the envelope and writing each run to
/// flash.
///
/// Split out of [`ota_upload_route`] so that function is the *checks* and this
/// is the *loop*, which is the only honest way to keep both readable: the
/// ordering of the checks is the safety argument, and a 155-line function does
/// not let a reader see it.
///
/// # The memory claim, at the one place it happens
///
/// `buf` is the **only** per-upload memory and it is a stack array. The sink
/// closure writes straight through to `esp_ota_write`, so no chunk is ever held
/// twice, and `PartReader`'s hold-back is a `Vec` whose capacity is reached on
/// the first push and never grows. A 1.6 MB image therefore costs the same heap
/// as a 4 KB one — which is the difference between this endpoint and an OOM
/// abort on a part with ~320 KB.
///
/// # Why a flash error does not unwind
///
/// The sink is `FnMut`, so it cannot return. A flash failure is recorded in
/// `failure` and the loop stops on the next chunk; `panic!` inside the closure
/// would unwind through `esp_ota_write`'s C frame, which is not a thing Rust can
/// do safely.
fn ota_stream(
    session: &crate::ota::Session,
    writer: &mut crate::ota::Writer,
    reader: &mut cc_net::web::ota::PartReader,
    req: &mut Request<&mut EspHttpConnection<'_>>,
) -> Result<(), OtaFailure> {
    let mut buf = [0u8; crate::ota::OTA_CHUNK_BYTES];
    let failed = false;
    loop {
        let read = match req.connection().read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut flash_error = None;
        let parsed = reader.push(&buf[..read], &mut |run| {
            if failed || flash_error.is_some() {
                return;
            }
            if let Err(err) = writer.write(run) {
                error!(
                    "ota: flash write failed at byte {}: {err}",
                    writer.written()
                );
                flash_error = Some(err);
            }
        });
        if let Err(err) = parsed {
            return Err(OtaFailure {
                status: 400,
                message: cc_net::web::ota::StatusMessage::Read(err),
                text: err.message(),
            });
        }
        if flash_error.is_some() {
            return Err(OtaFailure {
                status: 500,
                message: cc_net::web::ota::StatusMessage::Flash,
                text: "Write failed",
            });
        }
        // 0 rather than a guess for `total`: a browser's `Content-Length` covers
        // the multipart envelope, so it is not the image length. Only bytes
        // received are reported.
        session.note_progress(cc_net::web::ota::Phase::Uploading, writer.written(), 0);
    }
    Ok(())
}

/// Everything that can only be checked once the body has ended.
///
/// Four checks, in this order, and the order is the argument:
///
/// 1. **Termination.** The closing delimiter arrived. Checked first because a
///    truncated body makes every later check meaningless.
/// 2. **The extension rule**, on the filename the part headers carried. After
///    termination, because the filename is only known once the header block has
///    been read, and the payload had to be consumed either way to answer the
///    request.
/// 3. **The size rule**, against the partition — *after* the erase, which is the
///    only order available and the reason the `min_accepted` floor exists at
///    all: it cannot save the erase, only refuse to finalise.
///
/// # Why the extension check is not before the erase
///
/// It would be nicer. It cannot be: a `multipart/form-data` filename lives in the
/// part's headers, and the headers arrive *inside* the body, so the only way to
/// learn it is to read at least that far.
fn ota_validate(
    reader: &cc_net::web::ota::PartReader,
    written: usize,
    kind: Kind,
) -> Result<(), OtaFailure> {
    if let Err(err) = reader.finish() {
        return Err(OtaFailure {
            status: 400,
            message: cc_net::web::ota::StatusMessage::Read(err),
            text: err.message(),
        });
    }
    if let Some(name) = reader.filename() {
        if !cc_net::web::ota::extension_allowed(name, kind) {
            return Err(OtaFailure {
                status: 400,
                message: cc_net::web::ota::StatusMessage::BadExtension(kind),
                text: cc_net::web::ota::StatusMessage::BadExtension(kind).message(),
            });
        }
    }
    if !cc_net::web::ota::fits(kind, written) {
        return Err(OtaFailure {
            status: 400,
            message: cc_net::web::ota::StatusMessage::BadSize(kind),
            text: cc_net::web::ota::StatusMessage::BadSize(kind).message(),
        });
    }
    Ok(())
}

/// The refusal text for an OTA request the control task never answered.
///
/// Its own message rather than a `FlashRefusal`'s, because there is no state to
/// blame: the control task did not get to a decision within
/// [`COMMAND_ACK_TIMEOUT_MS`], or the command was lost. The UI prints
/// `result.message` verbatim, so it is operator-facing text and not a log line,
/// and it says what to do about it.
const NO_ANSWER_REFUSAL: &str =
    "The machine did not confirm a safe state for the update. Try again.";

/// The refusal text for a session another upload already holds.
const SESSION_BUSY_REFUSAL: &str =
    "OTA update already in progress. Please wait for current update to complete.";

/// Checks 1–3 of [`ota_upload_route`], in order: admission on the published
/// snapshot, the claim, and the safe hardware shutdown the control task applies
/// against its **live** state.
///
/// Split out of the route because the three together are the safety argument, and
/// the route is otherwise over [`clippy::too_many_lines`]. Returns the
/// operator-facing refusal text, or `Ok(())` when the flash may begin — which is
/// to say, once the control task has confirmed against the state the machine is
/// actually in that a `SafeHardwareShutdown` is now applied.
fn ota_open_session(
    session: &crate::ota::Session,
    shared: &Arc<Shared>,
    send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
    kind: Kind,
) -> Result<(), &'static str> {
    // 1. Admission, from the state the control task published. A snapshot is at
    //    most one control period stale, which is the same staleness every other
    //    read-only handler accepts — and it is why this is a *first filter* and
    //    not the decision. Check 3 is.
    let telemetry = shared.snapshot();
    // An id the firmware does not know is treated as "not safe to flash", which
    // is the conservative direction: an unrecognised state might be one that
    // flows water. `from_id`'s `None` has to mean something, and refusal is what
    // it means here.
    let admission = match u16::try_from(telemetry.machine_state)
        .ok()
        .and_then(cc_domain::state::MachineState::from_id)
    {
        Some(state) => cc_machine::ota::admit(state),
        None => cc_machine::ota::Admission::Refused(cc_machine::ota::FlashRefusal::FlowActive),
    };
    if let cc_machine::ota::Admission::Refused(refusal) = admission {
        info!("ota: refused — {}", refusal.message());
        return Err(refusal.message());
    }

    // 2. One at a time. Claimed **before** the shutdown so a second request sees
    //    a busy session rather than racing this one into the shutdown.
    if !session.claim(kind) {
        return Err(SESSION_BUSY_REFUSAL);
    }

    // 3. S8, and the wait that makes it mean something. The pump, the valve and
    //    the heater go off through the applier, on the control task, before
    //    `Writer::begin` erases anything — and the control task re-reads the
    //    **live** state on its way, because the snapshot above can be a tick
    //    stale and the machine is still running.
    let before = shared.applied();
    send(Command::OtaBegin);
    // `wait_applied` is the existing ack every command route uses, and it is
    // honest here for the same reason it is there: the control task notes the
    // command applied only after the telemetry a caller would read is published
    // (`main.rs`, step 8), so a settled wait means the verdict is in place.
    shared.wait_applied(before);

    // `None` covers both "the control task refused" (it would have written a
    // reason) and "the control task never answered", and both are the same
    // answer to this route's only question, which is whether `esp_ota_begin`
    // may run. No answer, no flash.
    let Some(admission) = session.take_verdict() else {
        session.finish_err(cc_net::web::ota::StatusMessage::Refused(NO_ANSWER_REFUSAL));
        return Err(NO_ANSWER_REFUSAL);
    };
    if let crate::ota::Admission::Refused(refusal) = admission {
        info!("ota: refused — {}", refusal.message());
        session.finish_err(cc_net::web::ota::StatusMessage::Refused(refusal.message()));
        return Err(refusal.message());
    }
    Ok(())
}

/// OTA upload ordering: snapshot admission, exclusive claim, acknowledged live
/// admission/shutdown, multipart boundary, slot erase, then streaming chunks.
///
/// The snapshot is only a first filter. Wait for control to recheck its state
/// after normal effects and apply shutdown separately. Refusal returns 409;
/// timeout means no flash. Never call `esp_ota_begin` before that acknowledgement.
/// Stream through a 4 KiB stack buffer without allocating per chunk.
pub(super) fn ota_upload_route(
    session: &crate::ota::Session,
    shared: &Arc<Shared>,
    send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
    kind: Kind,
    mut req: Request<&mut EspHttpConnection<'_>>,
) -> Result<(), EspError> {
    // 1-3. Admission on the published snapshot, the one-at-a-time claim, and the
    //    safe hardware shutdown the control task applies against its LIVE state.
    //    `Err` is the refusal text, and it is already logged and — where a
    //    session had been claimed — recorded on the status document.
    if let Err(refusal) = ota_open_session(session, shared, send, kind) {
        return respond(req.connection(), 409, &upload_response(false, refusal));
    }

    // 4. The multipart boundary. Read before the flash is touched, because a
    //    request with no envelope is a client error and must not cost an erase.
    // Copied because `req.header` borrows the connection the loop below then
    // reads from, and the borrow checker is right that the header does not
    // outlive it.
    let content_type: Option<String> = req.header("Content-Type").map(String::from);
    let Some(boundary) = cc_net::web::ota::PartReader::boundary_of(content_type.as_deref()) else {
        session.finish_err(cc_net::web::ota::StatusMessage::Read(
            cc_net::web::ota::ReadError::NoPart,
        ));
        return respond(
            req.connection(),
            400,
            &upload_response(false, "No firmware file provided"),
        );
    };
    let mut reader = cc_net::web::ota::PartReader::new(boundary);

    // 5. The erase.
    let mut writer = match crate::ota::Writer::begin(kind) {
        Ok(writer) => writer,
        Err(err) => {
            session.finish_err(cc_net::web::ota::StatusMessage::Flash);
            error!("ota: could not open the {kind:?} slot: {err}");
            return respond(
                req.connection(),
                500,
                &upload_response(false, "Failed to begin update"),
            );
        }
    };

    // 6. Stream.
    if let Err(outcome) = ota_stream(session, &mut writer, &mut reader, &mut req) {
        session.finish_err(outcome.message);
        writer.abort();
        return respond(
            req.connection(),
            outcome.status,
            &upload_response(false, outcome.text),
        );
    }

    // A truncated body is refused rather than finalised: `esp_ota_end` would
    // validate a partial image, and a half-written slot is worse than an erased
    // one because the erase has already happened.
    if let Err(failure) = ota_validate(&reader, writer.written(), kind) {
        session.finish_err(failure.message);
        writer.abort();
        return respond(
            req.connection(),
            failure.status,
            &upload_response(false, failure.text),
        );
    }

    let written = writer.written();

    // 8. Finalise. For firmware this is where `esp_ota_end` validates the image
    //    and switches the boot partition.
    match writer.end() {
        Ok(()) => {
            info!("ota: {kind:?} update complete — {written} B");
            session.note_progress(cc_net::web::ota::Phase::Processing, written, written);
            session.finish_ok();
            respond(
                req.connection(),
                200,
                &upload_response(true, "Update successful. Device will restart."),
            )
        }
        // **Which of the two failures this is matters.** `esp_ota_end`
        // rejecting the image and the `otadata` write that selects it failing
        // are different problems, and collapsing them into "invalid firmware
        // image" tells an operator whose flash is failing to re-upload a good
        // file — a symptom identical to the bug the slot selection was added to
        // fix. `StatusMessage::Flash` already exists for the second.
        Err(err @ (crate::ota::EndError::Image(_) | crate::ota::EndError::SelectSlot(_))) => {
            let status = match err {
                crate::ota::EndError::Image(_) => cc_net::web::ota::StatusMessage::Invalid,
                crate::ota::EndError::SelectSlot(_) => cc_net::web::ota::StatusMessage::Flash,
            };
            session.finish_err(status);
            error!("ota: {kind:?} finalise failed — {err}");
            respond(
                req.connection(),
                500,
                &upload_response(false, status.message()),
            )
        }
    }
}
