# Publish to npm

Use GitHub Actions to publish the precompiled `baseballhour` package. Users need Node.js and npm to install it. They need no npm account, API key, or Rust compiler.

## Connect GitHub to npm

The package uses npm trusted publishing. GitHub authenticates with a short-lived identity credential, so the repository needs no `NPM_TOKEN` secret.

To recreate the trusted publisher, use npm 11.15 or later with an account that has write access to `baseballhour`:

```sh
npm login
npm trust github baseballhour --repo justinramos101/baseballhour \
	--file npm-publish.yml --allow-publish --yes
```

Complete each browser authentication challenge that npm presents. To inspect the configuration, run:

```sh
npm trust list baseballhour
```

Confirm that the repository is `justinramos101/baseballhour` and the workflow filename is `npm-publish.yml`. Leave the environment name empty. The filename is case-sensitive.

Trusted publishing requires an existing npm package. The initial `baseballhour` publication used the local CLI. See [npm's trusted-publisher prerequisites](https://docs.npmjs.com/cli/v11/commands/npm-trust/#prerequisites) for new packages.

## Publish a release

Push a stable version tag after the source version and release notes are ready. The native Release workflow builds the four binaries and publishes the GitHub release. It then starts `npm-publish.yml` explicitly. A stable release published through the GitHub interface also starts the npm workflow.

To publish an existing release, run:

```sh
gh workflow run npm-publish.yml --repo justinramos101/baseballhour --ref main \
	-f tag=v0.1.0 -f publish=true
```

The npm workflow packages one tarball and verifies that same tarball on macOS and Linux, on x86_64 and ARM64. Publication runs after every verifier passes. The npm credential is available only to the final publication step.

## Verify without publishing

To run the complete packaging and verification pipeline without an npm credential, run:

```sh
gh workflow run npm-publish.yml --repo justinramos101/baseballhour --ref main \
	-f tag=v0.1.0 -f publish=false
```

Inspect the run in [GitHub Actions](https://github.com/justinramos101/baseballhour/actions/workflows/npm-publish.yml). Confirm that all four verification jobs pass.

## Retry a failed publication

Correct the failed check or publisher credential, then dispatch the workflow again with the same tag. Do not move the tag or replace the native release.

An existing npm version succeeds only when its published integrity matches the tested tarball. Different bytes fail. npm versions cannot be overwritten. If package contents must change, publish a new version.

GitHub serializes publication jobs. A newer pending job can replace an older pending job. If a run is cancelled before publication, dispatch that tag again.

## Check the package locally

Use Python 3.11 or later, npm, and the GitHub CLI to reproduce packaging for a published release:

```sh
python3 scripts/package_npm.py --tag v0.1.0
python3 scripts/verify_npm.py dist/npm/baseballhour-0.1.0.tgz \
	--release-manifest dist/npm/release.json --evidence dist/npm/evidence.json
```

The builder resolves checksums from the selected release and verifies native archive provenance. It writes generated metadata beside the tarball. To use downloaded archives, add `--assets DIRECTORY`.

To verify the checked-in release fixture used by pull-request CI, omit `--tag`:

```sh
python3 scripts/package_npm.py
python3 scripts/verify_npm.py
```

The fixture in `npm/release.json` selects an existing release independently of the current Rust source version. Automatic publication resolves each release without editing that fixture.

## Check the public package

From outside the source checkout, run:

```sh
npx --yes baseballhour@0.1.0 --version
npx --yes baseballhour --demo
```

Confirm the version and repository link on [npm](https://www.npmjs.com/package/baseballhour). For permanent installation, run `npm install --global baseballhour`.
