/*
 * `format_fixed` reference data generator.
 *
 * Emits 80 000 lines of `<f64 bit pattern> <decimals> <snprintf("%.*f")>` from
 * a fixed PRNG, covering decimals 0..=3. The output is committed as
 * `tests/fmt_oracle.txt` and asserted by `tests/fmt_parity.rs`.
 *
 * Not built by the test: the text file IS the artefact. This program is kept so
 * the artefact can be re-derived if C's rounding rule is ever in question.
 *
 *   cc -O0 -o /tmp/fmt_oracle crates/cc-display/tools/fmt_oracle.c
 *   /tmp/fmt_oracle > crates/cc-display/tests/fmt_oracle.txt
 *
 * -O0 so nothing in the compiler's constant folding can change a conversion.
 * The values are carried as raw bits (`memcpy`-equivalent via a union, which C
 * guarantees) so the Rust side reconstructs the identical double rather than
 * re-deriving it from a decimal literal.
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define CASES_PER_PRECISION 20000

int main(void) {
    /* A 64-bit LCG: deterministic across machines and compilers, which is what
     * makes the committed file meaningful. */
    uint64_t state = 12345;
    for (int decimals = 0; decimals <= 3; decimals++) {
        for (int i = 0; i < CASES_PER_PRECISION; i++) {
            state = state * 6364136223846793005ULL + 1442695040888963407ULL;
            const double unit = (double)((state >> 11) & 0xFFFFFFFFFFFFFULL) / (double)0x10000000000000ULL;
            const double value = unit * 200.0 - 100.0;

            uint64_t bits;
            memcpy(&bits, &value, sizeof bits);

            char rendered[64];
            snprintf(rendered, sizeof rendered, "%.*f", decimals, value);
            printf("%016llx %d %s\n", (unsigned long long)bits, decimals, rendered);
        }
    }
    return 0;
}
