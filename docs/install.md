# Install Baseball Hour

## Run with npx

With [Node.js and npm](https://nodejs.org/en/download) installed, run:

```sh
npx baseballhour
```

npx downloads the package into npm's cache and starts the app. It may ask you to confirm the first download. You do not need a global installation, an account, an API key, or a Rust compiler.

Try the offline demo:

```sh
npx baseballhour --demo
```

Arguments after `baseballhour` go to the app. For example:

```sh
npx baseballhour --team Yankees
npx baseballhour --league all --near Denver
```

To select a version, include it in the package name:

```sh
npx baseballhour@0.1.0 --demo
```

## Install with npm

To keep the `baseballhour` command installed, run:

```sh
npm install -g baseballhour
baseballhour
```

The package includes precompiled executables for all four [supported platforms](#supported-native-archives). npm installs the package without build tools or installation scripts.

To update the global installation, run:

```sh
npm install -g baseballhour@latest
```

To remove it, run:

```sh
npm uninstall -g baseballhour
```

## Install the latest native binary

The installer supports macOS and Linux on Intel, AMD, and ARM64 processors. It does not require Rust or `sudo`.

Download the installer, inspect it, and run it:

```sh
curl -fsSLO https://raw.githubusercontent.com/justinramos101/baseballhour/main/scripts/install.sh
chmod +x install.sh
./install.sh
```

The installer detects your platform, downloads the matching GitHub release archive, verifies its SHA-256 checksum, and installs `baseballhour` in `$HOME/.local/bin`.

Add the installation directory to `PATH` if your shell cannot find the command. For Bash or Zsh, add this line to your shell configuration:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Start the synthetic offline tour to verify the installation:

```sh
baseballhour --demo
```

You can also run the installer in one command:

```sh
curl -fsSL https://raw.githubusercontent.com/justinramos101/baseballhour/main/scripts/install.sh | sh
```

## Choose a version or prefix

The installer uses the latest GitHub release by default. Pass a release tag to install a specific version:

```sh
./install.sh --version v0.1.0
```

Pass `--prefix` to install somewhere other than `$HOME/.local`:

```sh
./install.sh --prefix "$HOME/.local/baseballhour"
"$HOME/.local/baseballhour/bin/baseballhour" --demo
```

Pass installer arguments after `--` when you pipe the script to `sh`:

```sh
curl -fsSL https://raw.githubusercontent.com/justinramos101/baseballhour/main/scripts/install.sh | sh -s -- --version v0.1.0 --prefix "$HOME/.local"
```

The installer never invokes `sudo`.

## Supported native archives

| System | Processor | Release target |
| --- | --- | --- |
| Linux | Intel or AMD 64-bit | `x86_64-unknown-linux-musl` |
| Linux | ARM64 | `aarch64-unknown-linux-musl` |
| macOS | Intel | `x86_64-apple-darwin` |
| macOS | Apple silicon | `aarch64-apple-darwin` |

Linux archives use musl and are statically linked.

Release binaries are not code-signed or notarized. If macOS blocks a downloaded binary, follow your organization's software policy or [build from source](#build-from-source).

## Install an archive manually

Open the [Baseball Hour releases](https://github.com/justinramos101/baseballhour/releases) page and download the archive for your system. Download its matching `.sha256` file too.

Verify the archive from the download directory. Replace the filename with the archive that you downloaded.

```sh
# Linux
sha256sum --check baseballhour-v0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256

# macOS
shasum -a 256 --check baseballhour-v0.1.0-aarch64-apple-darwin.tar.gz.sha256
```

Extract the archive and install the binary:

```sh
tar -xzf baseballhour-v0.1.0-aarch64-apple-darwin.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 755 baseballhour-v0.1.0-aarch64-apple-darwin/baseballhour "$HOME/.local/bin/baseballhour"
```

Run `baseballhour --demo` to check the result.

## Build from source

Install a current stable [Rust toolchain](https://www.rust-lang.org/tools/install) and a system C linker. On macOS, install the Xcode Command Line Tools. On Linux, install your distribution's C build tools.

Clone the repository and run the source installer:

```sh
git clone https://github.com/justinramos101/baseballhour.git
cd baseballhour
./scripts/install-source.sh --prefix "$HOME/.local"
```

The source installer builds the locked release binary with two parallel jobs and copies it to `PREFIX/bin/baseballhour`. Set `CARGO_BUILD_JOBS` to change the job limit.

You can also install through Cargo:

```sh
cargo install --path . --locked --jobs 2
```

Cargo installs the binary in `$CARGO_HOME/bin`, usually `$HOME/.cargo/bin`.

## Update or remove Baseball Hour

For an npm installation, use `npm install -g baseballhour@latest` to update or `npm uninstall -g baseballhour` to remove it.

For a native installer installation, run the installer again to update to the latest release. Pass `--version` to install a selected release.

To remove a native installation, delete the installed binary from its prefix:

```sh
rm "$HOME/.local/bin/baseballhour"
```

To remove a Cargo installation, run:

```sh
cargo uninstall baseballhour
```
