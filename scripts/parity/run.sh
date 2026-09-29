#!/usr/bin/env bash
#
# The parity harness runner (06 R1-08). `just parity <port> <host>` calls this.
#
#   scripts/parity/run.sh <port> <host>
#
# It runs every scenario in docs/rust-migration/scenarios/ against the firmware
# currently on the device, captures the observation each scenario declares, and
# diffs it against docs/rust-migration/baseline/cpp/. Any diff not explained by
# a ```ledger entry in docs/rust-migration/intentional-diffs.md is a regression
# and exits non-zero.
#
# WHAT THIS SCRIPT DOES NOT DO
# ----------------------------
# It does not flash anything, and it does not drive the pump, the valve or the
# heater. Both are deliberate:
#
#   * Flashing is a separate, explicit step. `just flash <port>` overwrites the
#     device, and the skill's rule 3 is that a flash needs `just identify` first.
#     A parity run that silently reflashed would destroy whatever firmware the
#     operator had on the machine, and the C++/Rust comparison is only
#     meaningful if the operator chose which image is under test.
#
#   * No actuator is energised by this script. The hardware scenarios in the set
#     are the three that cannot: cold boot, a sensor read, and the heater-gate
#     check, all with `pid.enabled: false`. See
#     docs/rust-migration/scenarios/heater_gate_closed.yaml for why a real
#     over-temperature test is NOT in the set.
#
# EXIT CODES
#   0  every scenario matched its C++ baseline, or explained every difference
#   1  an unexplained diff, or a scenario whose assertions failed
#   2  usage error, missing baseline, or an unreachable device
#
# CREDENTIALS
#   Wi-Fi credentials are read from the environment at run time
#   (CC_PARITY_WIFI_SSID / CC_PARITY_WIFI_PASS, falling back to WIFI_SSID /
#   WIFI_PASS) and are never echoed, never passed on a command line, and never
#   written to a file. If the device is not already on the network the script
#   says so and stops; it does not provision.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"

scenario_dir="${repo_root}/docs/rust-migration/scenarios"
baseline_dir="${repo_root}/docs/rust-migration/baseline"
ledger_file="${repo_root}/docs/rust-migration/intentional-diffs.md"
out_dir="${CC_PARITY_OUT:-${repo_root}/target/parity}"

die() {
    echo "run.sh: $*" >&2
    exit 2
}

note() { echo "run.sh: $*"; }

usage() {
    cat >&2 <<'EOF'
usage: scripts/parity/run.sh <port> <host>

  port   the device's serial port, e.g. /dev/cu.usbserial-204140
  host   the device's hostname on the network, e.g. clevercoffee.local

environment:
  CC_PARITY_SCENARIO   run only this scenario (by name)
  CC_PARITY_FIRMWARE   label for the observation, default "rust"
  CC_PARITY_RECORD     write the observation as the new baseline (see the
                       warning the script prints before doing it)
EOF
    exit 2
}

# ---------------------------------------------------------------- arguments

[ $# -eq 2 ] || usage
port="$1"
host="$2"

[ -d "${scenario_dir}" ] || die "no scenario directory at ${scenario_dir}"
[ -f "${ledger_file}" ] || die "no divergence ledger at ${ledger_file}"

firmware="${CC_PARITY_FIRMWARE:-rust}"
mkdir -p "${out_dir}"

# --------------------------------------------------------------- the binary

# `cc-parity` is a host crate: it links the reducer and a recording `Actuators`
# and has no GPIO. `cargo run -p` is slow enough that the scenarios are run in
# one invocation rather than one per file.
run_binary() {
    ( cd "${repo_root}" && cargo run --quiet -p cc-parity --target "${CC_HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}" -- "$@" )
}

note "building the harness"
run_binary --help >/dev/null 2>&1 || die "cannot build or run cc-parity"

command -v jq >/dev/null 2>&1 || die "jq is required (it unwraps the runner's JSON array)"

# ------------------------------------------------------- the divergence ledger

note "reading the divergence ledger"
if ! run_binary ledger "${ledger_file}" > "${out_dir}/ledger.json"; then
    die "the divergence ledger is unreadable or has drifted from intentional-diffs.md"
fi
ledger_entries="$(grep -c '"id"' "${out_dir}/ledger.json" || true)"
note "  ${ledger_entries} declared divergence(s)"

# ------------------------------------------------------------ device reachability

# The hardware scenarios need the device. A `dry_run` scenario does not, and the
# set is mostly those, so an unreachable device is a warning rather than an
# error — but it is reported, because silently skipping the hardware scenarios
# would make a parity run that passes mean less than it appears to.
device_reachable=0
if curl -sf -m 5 "http://${host}/api/health" >/dev/null 2>&1; then
    device_reachable=1
    note "device reachable at ${host}"
else
    note "WARNING: ${host} is not answering /api/health."
    note "         hardware scenarios will be reported as SKIPPED."
    note "         Credentials, if needed, come from the environment and are never logged:"
    note "           export CC_PARITY_WIFI_SSID=... CC_PARITY_WIFI_PASS=..."
fi

# ------------------------------------------------------------------ scenarios

# `mapfile` is bash 4+; macOS ships bash 3.2 as /bin/bash. Read into an array
# with a `while read` loop so the script runs on both.
scenarios=()
while IFS= read -r line; do
    scenarios+=("${line}")
done < <(ls -1 "${scenario_dir}"/*.yaml 2>/dev/null | sort)
[ "${#scenarios[@]}" -gt 0 ] || die "no scenarios in ${scenario_dir}"

failures=0
skipped=0
missing_baseline=0
checked=0
declare -a failed_names=()

for path in "${scenarios[@]}"; do
    name="$(basename "${path}" .yaml)"

    if [ -n "${CC_PARITY_SCENARIO:-}" ] && [ "${CC_PARITY_SCENARIO}" != "${name}" ]; then
        continue
    fi

    mode="$(run_binary list "${scenario_dir}" | awk -v n="${name}" -F'\t' '$1==n {print $2}')"
    [ -n "${mode}" ] || die "cannot read ${name} from the scenario set"

    if [ "${mode}" = "Hardware" ] && [ "${device_reachable}" -eq 0 ]; then
        echo "SKIP  ${name} (hardware, device unreachable)"
        skipped=$((skipped + 1))
        continue
    fi

    baseline="${baseline_dir}/cpp/${name}.json"
    observed="${out_dir}/${firmware}/${name}.json"
    # Created before the redirections below, which resolve the path at the time
    # the command runs.
    mkdir -p "$(dirname "${observed}")" "${out_dir}"

    # A `dry_run` scenario is driven entirely on the host by the harness. A
    # `hardware` scenario is not — driving the device is R4-03's job, once the
    # control task exists (06 R4-01) — so this script reports the scenario's
    # absence rather than pretending to have run it.
    if [ "${mode}" = "Hardware" ]; then
        echo "TODO  ${name} (hardware; capture lands with R4-01's control task)"
        skipped=$((skipped + 1))
        continue
    fi

    if ! run_binary run "${path}" > "${observed}.tmp" 2>"${out_dir}/${name}.err"; then
        echo "FAIL  ${name} (assertion)"
        sed 's/^/      /' "${out_dir}/${name}.err"
        failures=$((failures + 1))
        failed_names+=("${name}")
        rm -f "${observed}.tmp"
        continue
    fi
    # `cc-parity run` prints a JSON **array** (it can take several scenarios);
    # a baseline is one scenario's observation, so the array is unwrapped here
    # rather than by `diff`, which reads one observation.
    if ! jq '.[0]' < "${observed}.tmp" > "${observed}"; then
        echo "FAIL  ${name} (the observation is not readable)"
        failures=$((failures + 1))
        failed_names+=("${name}")
    fi
    rm -f "${observed}.tmp"

    if [ ! -f "${baseline}" ]; then
        # Not a skip. 06 §Definitions makes "parity on the committed scenario
        # set" the definition, and a scenario with no C++ reference has nothing
        # to be compared against — so the honest report is that the comparison
        # did not happen, and the exit code says so.
        echo "BASELINE-MISSING  ${name}"
        echo "      no ${baseline#"${repo_root}/"} — capture the C++ firmware's"
        echo "      observation for this scenario, or delete the scenario."
        missing_baseline=$((missing_baseline + 1))
        continue
    fi

    checked=$((checked + 1))
    if run_binary diff "${baseline}" "${observed}" "${ledger_file}"; then
        echo "OK    ${name}"
    else
        echo "FAIL  ${name} (unexplained diff)"
        failures=$((failures + 1))
        failed_names+=("${name}")
    fi
done

# --------------------------------------------------------------------- summary

echo
echo "-----------------------------------------------------------"
echo "scenarios run:     ${checked}"
echo "assertion failures: ${failures}"
echo "skipped:            ${skipped}"
echo "missing baselines:  ${missing_baseline}"
echo "-----------------------------------------------------------"

if [ -n "${CC_PARITY_RECORD:-}" ]; then
    cat <<EOF

REFUSING to overwrite the C++ baseline.

  CC_PARITY_RECORD is set, which would replace the C++ reference with the
  current firmware's output. That is the one operation this harness must never
  do unattended: it would make the comparison a tautology, and every
  behavioural change since the baseline was captured would be silently adopted
  as correct.

  To re-capture a baseline, flash the C++ firmware, re-run without
  CC_PARITY_RECORD, read the diff line by line, and commit the new
  baseline/cpp/<name>.json with the reasoning in the commit message.
EOF
    exit 2
fi

if [ "${failures}" -gt 0 ]; then
    echo
    echo "FAILED: ${failed_names[*]}"
    echo "Each one is either a regression, or a change that belongs in"
    echo "docs/rust-migration/intentional-diffs.md with a \`\`\`ledger entry."
    exit 1
fi

if [ "${missing_baseline}" -gt 0 ]; then
    echo
    echo "INCOMPLETE: ${missing_baseline} scenario(s) have no C++ baseline."
    echo "This is not a pass. 'Parity' is defined as equivalence with the C++"
    echo "firmware on the committed scenario set, and a scenario with no"
    echo "reference has not been compared with anything."
    exit 2
fi

exit 0
