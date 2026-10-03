# Maintain the npm package

The `baseballhour` npm package bundles precompiled binaries for macOS and Linux on x86_64 and ARM64. Installation runs no lifecycle scripts and requires no Rust compiler. The launcher replaces itself with the native application so keyboard input and terminal signals reach the TUI directly.

## Prepare a release

Publish the native GitHub release first. Update `npm/release.json` with its version, tag, and the SHA-256 of each of the four release archives. The package version and native release version must match. This table also generates the launcher's platform selection and package metadata.

Use Python 3.11 or later and an installed npm CLI to build the package:

```sh
python3 scripts/package_npm.py
python3 scripts/verify_npm.py
```

The builder downloads the selected GitHub release, verifies every archive checksum, and packs only the launcher, four binaries, README, license, and package manifest. Its output is `dist/npm/baseballhour-VERSION.tgz`. To reuse downloaded archives, pass `--assets DIRECTORY` to the builder.

The verifier installs that tarball into a temporary global prefix and runs it through a fresh npx cache. It checks arguments, JSON, symlinks, errors, navigation, resizing, quitting, and terminal restoration. It then removes the temporary global installation. It does not change your existing global packages.

Open a pull request and wait for the npm package workflow to pass on all four native platforms. The workflow tests the release selected by `npm/release.json`, independently of the current Rust source version.

## Publish and check

Authenticate with the npm account authorized to publish `baseballhour`. Check the account with `npm whoami`, then publish the tested tarball. For version 0.1.0:

```sh
npm publish dist/npm/baseballhour-0.1.0.tgz --access public --ignore-scripts
```

Complete any authentication challenge npm presents. Never commit registry credentials. Publishing is manual; the repository workflow has no npm token.

Check the public package from outside the source checkout:

```sh
npx --yes baseballhour@0.1.0 --version
npx --yes baseballhour --demo
```

Confirm the published version and repository link on [npm](https://www.npmjs.com/package/baseballhour). Merge the source pull request after publication succeeds so installation instructions point to an available package.
