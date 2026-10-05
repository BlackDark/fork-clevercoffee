# Contributing to CleverCoffee

Thank you for considering contributing to the project. To ensure consistency and maintainability, please follow these style
guidelines when submitting code changes.

## Before you start

This repository holds **one** firmware, written in Rust, in `crates/cc-*`. The
C++ firmware it replaced was deleted once the port became the product; start at
[docs/history/README.md](docs/history/README.md), and read
[divergences.md](docs/history/divergences.md) before changing
anything whose behaviour looks wrong -- it is a ledger of places the port
deliberately differs from the C++, with the reasoning for each.

The numbered rules are in [AGENTS.md](AGENTS.md) and are not restated here.

```sh
just setup     # once: mise tools, the Espressif Xtensa toolchain, the web UI
just check     # fmt, clippy, rustdoc, tests, parity -- needs no hardware
just gate      # the above plus device clippy, the firmware build, the size budget
```

`just check` is the gate a pull request has to pass; CI runs it in
`.github/workflows/rust.yml`. Do not commit until it is green (`AG-REPO-6`).

## Code Style Guidelines

### Formatting

`cargo fmt` is the formatter, and `rustfmt`'s defaults are the style -- there is
no `rustfmt.toml` to disagree with them. Apply it with `just fmt`, check it with
`just fmt-check`; CI runs the check.

[`pre-commit`](https://pre-commit.com/) is available for the generic hygiene
hooks (trailing whitespace, end-of-file newline, merge conflicts):

```bash
$ pip install pre-commit
$ pre-commit install
```

There is deliberately **no** `cargo fmt` pre-commit hook: a `language: system`
hook runs with pre-commit's own PATH, which does not contain the project's cargo,
so every commit on a Rust file would report "Executable `cargo` not found". A
hook that cannot run teaches people to pass `--no-verify`. `just fmt-check` is
the single gate, and CI enforces it where it cannot be skipped.

### What the lints enforce for you

`just lint` runs clippy with `clippy::pedantic` as **deny**, and rustdoc runs
with `-D warnings`. Read those two before adding an `#[allow]` or an `#[expect]`
(`AG-RUST-10`) -- every one already in the tree is there for a reason.

Comments should explain the **why**, not restate the code. Write for the
maintainer who does not know the subsystem.

## Submitting Changes

1. Fork the repository and create a new branch for your changes.
2. Ensure your code follows the outlined style guidelines.
3. Make sure to include clear commit messages explaining the purpose of the changes.
4. Open a pull request with a descriptive title and detailed information about the changes made.
5. **Choose the correct target branch for your pull request:**  
   - For new features or improvements, open the PR against the `develop` branch.  
   - If the bug is in `develop`, target the `develop` branch.
   - If fixing a bug found in `main`, open the PR against `main`.  

## Code Review Process

All contributions will be reviewed to ensure compliance with the project's guidelines. Be prepared to address any feedback or suggestions for improvement during the review process.

Thank you for your contributions to CleverCoffee!
