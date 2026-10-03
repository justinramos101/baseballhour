# Contribute to Baseball Hour

Baseball Hour is a Rust terminal application. Contributions should preserve honest data labels, keyboard access, and usable layouts across terminal sizes.

## Set up the repository

Install a current stable Rust toolchain, a system C linker, Python 3.11 or later, and Git. Then clone and build the repository:

```sh
git clone https://github.com/justinramos101/baseballhour.git
cd baseballhour
cargo build --locked
cargo run --locked -- --demo
```

Demo mode provides a repeatable schedule without network access.

For distribution changes, follow the [npm package maintenance guide](docs/npm.md).

## Find the owning module

Read [module ownership](docs/architecture.md#module-ownership) before a change that crosses modules. The main ownership boundaries are:

- `src/model.rs` owns domain types.
- `src/data.rs` owns provider parsing, network access, cache behavior, and demo data.
- `src/app.rs` owns interaction state and preferences.
- `src/ui.rs` and its submodules own terminal layouts and copy.
- `src/storage.rs` owns atomic file replacement.
- `src/render.rs` owns text and SVG exports.
- `src/main.rs` owns the CLI, worker, refresh loop, and terminal lifecycle.

Keep provider input handling in `data.rs`. Keep view and keyboard behavior in `app.rs`. Keep layout decisions in `ui.rs`.

## Run the checks

Run the same Rust checks used by continuous integration:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Clippy's full `pedantic` group and the `dbg_macro`, `todo`, and `unimplemented`
lints are enabled in `Cargo.toml`; CI treats warnings as errors. First-party
unsafe code is forbidden. Keep any lint exceptions local and include a reason.

Install the pinned dependency tools and run the dependency checks:

```sh
cargo install cargo-deny --version 0.19.4 --locked
cargo install cargo-machete --version 0.9.2 --locked
cargo deny --locked check licenses bans sources
cargo machete
```

`deny.toml` defines the license and source policy for the supported targets.
Review policy changes when adding dependencies. Duplicate transitive versions
produce warnings; disallowed licenses, sources, and wildcard dependencies fail
the check. Security advisories remain covered by the separate cargo-audit job.

For changes to input, resizing, or terminal cleanup, run the pseudo-terminal check:

```sh
python3 scripts/verify_terminal.py
```

For changes to visible output, regenerate the committed captures:

```sh
cargo run --locked --example screenshots -- docs/screenshots
```

Review every changed `.svg` and `.txt` file in `docs/screenshots`. Commit capture changes only when the interface change is intentional.

## Submit a pull request

Keep each pull request focused on one problem. Include the user-visible result, the checks that you ran, and any platform behavior that you could not test. Add tests for behavior that can regress.

Do not include credentials, private schedule data, local cache files, or local configuration files. Use the [private security advisory form](https://github.com/justinramos101/baseballhour/security/advisories/new) for a security report.
