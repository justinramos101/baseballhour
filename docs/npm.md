# Publish to npm

Use GitHub Actions to publish the precompiled `baseballhour` package. Users need Node.js and npm to install it. They need no npm account, API key, or Rust compiler.

## Connect the npm account

For the first publication, add a token to GitHub:

1. Open [npm access tokens](https://www.npmjs.com/settings/thulr/tokens) while signed in as `thulr`.
2. Create a granular token with **Read and write (publish and stage)** permission.
3. Enable **Bypass 2FA** for unattended publication.
4. Select **All Packages** because `baseballhour` does not exist yet.
5. Set a short expiration date.
6. Copy the token into the repository's [new Actions secret form](https://github.com/justinramos101/baseballhour/settings/secrets/actions/new).
7. Name the secret `NPM_TOKEN`.

Keep the token out of source files, issue comments, and chat. After the first publication, replace the token with trusted publishing or a token restricted to `baseballhour`.

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

## Remove the publishing token

After `baseballhour` exists on npm, configure [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/):

1. Open the package settings on npm.
2. Add a GitHub Actions trusted publisher.
3. Set the repository owner to `justinramos101`.
4. Set the repository name to `baseballhour`.
5. Set the workflow filename to `npm-publish.yml`.
6. Leave the environment name empty.
7. Delete the GitHub `NPM_TOKEN` secret after the trusted publisher is configured.

The workflow supports npm's short-lived GitHub identity credentials and attaches package provenance. Trusted publishing requires an existing package, so the first publication uses the token.

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
