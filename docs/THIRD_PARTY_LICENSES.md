# Third-party licences

This repository is GPL-3.0-or-later. It also **redistributes** bytes and code
from third-party projects. BSD-2-Clause §3 and the Apache-2.0 §4 both require
that the upstream notice travels with the redistribution, so this file is that
notice.

Anything listed here is also covered by the corresponding in-tree file header,
so a reader who only ever opens one source file still sees the obligation.

---

## U8g2 — BSD 2-Clause

**Where the bytes are used**

| File | What |
| --- | --- |
| `crates/cc-display/src/font/data.rs` | 2,807 lines of **verbatim** U8g2 RLE font streams (10 fonts: `profont10/11/12/15/17/22`, `fub17/20/25/30`) |
| `crates/cc-display/src/font/mod.rs` | A hand-written decoder for that RLE stream, written for this repository. **Original work, not U8g2 code** — no notice obligation, but listed so the boundary is explicit. |
| `crates/cc-display/tools/oracle/U8g2Shim.cpp` | A shim that lets the *real* U8g2 library run on the host, used by `just test-display-parity`. |

**How the bytes got here**

`crates/cc-display/tools/extract_fonts.py` reads the `u8g2_fonts.c` that the C++
firmware links (`platformio.ini`, `lib_deps: olikraus/U8g2 @ 2.36.18`) and emits
the arrays byte-for-byte. Regenerate with:

```sh
crates/cc-display/tools/oracle/run.sh          # rebuild data.rs
crates/cc-display/tools/oracle/run.sh --check  # fail if data.rs has drifted
```

`extract_fonts.py` emits this notice in the generated header, so a regeneration
cannot silently drop it.

**Upstream notice**

```
Copyright (c) 2014, olikraus@gmail.com
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

Neither the project name nor any contributor name is used to endorse this
firmware, so the BSD-2-Clause "no endorsement" clause is satisfied.

---

## Arduino-PID-Library — MIT

`lib/Arduino-PID-Library/` holds the C++ parity oracle's PID implementation
(`PID_v1.cpp` / `PID_v1.h`), unchanged. The Rust port in
`crates/cc-domain/src/pid.rs` was written for this repository and is verified
against that library by `crates/cc-domain/tools/pid_oracle/`, which
**executes the original MIT-licensed code** and diffs its output against the
Rust. The MIT notice travels with the vendored source in `lib/`.

---

## Not vendored

- **React, the UI toolchain and the browser.** `ui/` builds against published
  npm packages; their licences are in the packages themselves and none of them
  is redistributed here.
- **The C++ framework.** The parity oracle keeps `src/`, `include/` and `test/`
  which depend on the Arduino core and PlatformIO libraries. They are **not
  vendored into this repository** — PlatformIO fetches them at build time — so
  their notices are not ours to carry.
