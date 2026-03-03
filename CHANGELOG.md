# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.5](https://github.com/kantord/blogtato/compare/v0.1.4...v0.1.5) - 2026-03-03

### Added

- track read/unread status for articles ([#41](https://github.com/kantord/blogtato/pull/41))

### Fixed

- avoid making empty commits ([#40](https://github.com/kantord/blogtato/pull/40))
- support slightly malformed timezones ([#38](https://github.com/kantord/blogtato/pull/38))

### Other

- move git logic into db

## [0.1.4](https://github.com/kantord/blogtato/compare/v0.1.3...v0.1.4) - 2026-03-03

### Fixed

- git operations sometimes hang ([#35](https://github.com/kantord/blogtato/pull/35))

### Other

- do not rely on comments to structure code ([#37](https://github.com/kantord/blogtato/pull/37))

## [0.1.3](https://github.com/kantord/blogtato/compare/v0.1.2...v0.1.3) - 2026-03-02

### Added

- allow filtering by date ([#30](https://github.com/kantord/blogtato/pull/30))
- introduce free-combination grouping syntax ([#28](https://github.com/kantord/blogtato/pull/28))

### Other

- explain git based sync decision ([#34](https://github.com/kantord/blogtato/pull/34))
- update documentation to reflect new filtering logic ([#33](https://github.com/kantord/blogtato/pull/33))
- *(deps)* update rust crate rstest to 0.26 ([#32](https://github.com/kantord/blogtato/pull/32))
- make test code less verbose ([#31](https://github.com/kantord/blogtato/pull/31))

## [0.1.2](https://github.com/kantord/blogtato/compare/v0.1.1...v0.1.2) - 2026-03-01

### Added

- truncate columns to fit terminal width ([#25](https://github.com/kantord/blogtato/pull/25))
- pad shorthands in output ([#23](https://github.com/kantord/blogtato/pull/23))

### Fixed

- *(deps)* update rust crate terminal_size to v0.4.3 ([#26](https://github.com/kantord/blogtato/pull/26))
- support unicode truncation properly ([#27](https://github.com/kantord/blogtato/pull/27))

## [0.1.1](https://github.com/kantord/blogtato/compare/v0.1.0...v0.1.1) - 2026-03-01

### Added

- add some color to output ([#22](https://github.com/kantord/blogtato/pull/22))

### Fixed

- defensively use a more flexible db location ([#21](https://github.com/kantord/blogtato/pull/21))

### Other

- *(deps)* update actions/checkout action to v6 ([#19](https://github.com/kantord/blogtato/pull/19))
- *(deps)* pin dependencies ([#18](https://github.com/kantord/blogtato/pull/18))
