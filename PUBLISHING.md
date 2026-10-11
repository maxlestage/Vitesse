# Publishing on crates.io

Vitesse is published on [crates.io](https://crates.io/crates/vitesse) as the `vitesse`
crate, under its [proprietary license](LICENSE): the source is visible there so that it can
be audited and built, and anyone may depend on it.

## One release = one tag

1. Bump `version` in [`Cargo.toml`](Cargo.toml), update [`CHANGELOG.md`](CHANGELOG.md) and
   commit.
2. Open **Releases → Draft a new release**, create the tag `vX.Y.Z` (the same version with a
   `v` in front) and tap **Publish release**.

The [`release.yml`](.github/workflows/release.yml) workflow checks that the tag matches the
version, runs the tests and publishes the crate. It works from the GitHub website on a phone;
nothing is installed locally. From the **Actions** tab it can also be started by hand, as a
dry run by default (the package is built and checked, nothing is uploaded).

## One-time setup

- A crates.io account with a verified e-mail address.
- A token from [crates.io/settings/tokens](https://crates.io/settings/tokens) with the scopes
  **publish-new** and **publish-update**, saved as the repository secret
  `CARGO_REGISTRY_TOKEN` (GitHub → Settings → Secrets and variables → Actions).

## From a computer

```sh
cargo login                 # paste the token, once
cargo publish --dry-run     # packages and builds exactly what would be uploaded
cargo publish
```

## The ecosystem, in order

The six projects by Maxime Nathan Lestage depend on each other through crates.io, so they
are published in this order, each one after the previous one is online:

1. **Active** — `active`
2. **Vitesse** — `vitesse` (this repository)
3. **Securite** — `securite`
4. **Memoire** — `memoire-orm-derive`, then `memoire-orm`
5. **Paquets** — `paquets-macros`, then `paquets` (depends on the four above)
6. **FerroSwift** — `ferroswift-macros`, `ferroswift-transpiler`, `ferroswift`, `ferroswift-cli`
