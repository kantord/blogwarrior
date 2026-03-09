# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.14](https://github.com/kantord/blogtato/compare/v0.1.13...v0.1.14) - 2026-03-09

### Fixed

- fetch posts also from new feeds coming from remote ([#100](https://github.com/kantord/blogtato/pull/100))

## [0.1.13](https://github.com/kantord/blogtato/compare/v0.1.12...v0.1.13) - 2026-03-08

### Added

- allow exporting feed list to opml ([#95](https://github.com/kantord/blogtato/pull/95))

## [0.1.12](https://github.com/kantord/blogtato/compare/v0.1.11...v0.1.12) - 2026-03-08

### Added

- prevent data loss due to old versions after future migrations ([#93](https://github.com/kantord/blogtato/pull/93))
- allow importing opml files ([#92](https://github.com/kantord/blogtato/pull/92))

### Other

- fix typo: s/you likely has/you likely have/ ([#90](https://github.com/kantord/blogtato/pull/90))
- fix markdown formatting ([#88](https://github.com/kantord/blogtato/pull/88))
- mention subscribing to blogtato releases ([#86](https://github.com/kantord/blogtato/pull/86))

## [0.1.11](https://github.com/kantord/blogtato/compare/v0.1.10...v0.1.11) - 2026-03-08

### Added

- mark old posts as read when subscribing to feed ([#83](https://github.com/kantord/blogtato/pull/83))

### Other

- *(deps)* update rust crate libc to v0.2.183 ([#85](https://github.com/kantord/blogtato/pull/85))

## [0.1.10](https://github.com/kantord/blogtato/compare/v0.1.9...v0.1.10) - 2026-03-07

### Added

- easily merge remote and local db ([#81](https://github.com/kantord/blogtato/pull/81))
- remove redundant date filter

### Fixed

- fix failing test ([#82](https://github.com/kantord/blogtato/pull/82))

## [0.1.9](https://github.com/kantord/blogtato/compare/blogtato-v0.1.8...blogtato-v0.1.9) - 2026-03-07

### Added

- *(synctato)* add Table::delete_where() ([#70](https://github.com/kantord/blogtato/pull/70))
- *(synctato)* add Table::iter() for zero-clone iteration

### Other

- improve blog sync performance ([#76](https://github.com/kantord/blogtato/pull/76))
- *(deps)* update release-plz/action digest to 1528104 ([#74](https://github.com/kantord/blogtato/pull/74))
- add performance measuring scripts ([#75](https://github.com/kantord/blogtato/pull/75))
- avoid per-item SHA-256 hashing in read status filter ([#73](https://github.com/kantord/blogtato/pull/73))
- avoid allocations in GroupKey sort comparator
- deduplicate shorthand lookup in FeedIndex
- extarct MAX_FEED_CANDIDATES
- deduplicate spinner creation
- extract BLOG_NAME_BUDGET_PERCENT
- explain shorthand logic and design a bit
- replace ensure_no_query with reject_filter to avoid unnecessary parsing
- move query execution logic from data/index to query/resolve
- replace tautological assertion
- small fixes
- extract start_of_day helper
- use partition in split_at_command
- extract Query::or_default_view
- replace parallel vectors in FeedIndex with Vec<FeedEntry>
- reuse Style between group.rs and item.rs
- break down format_item into smaller bits
- accept &RenderCtx in render_grouped instead of 7 positional args

## [0.1.8](https://github.com/kantord/blogtato/compare/blogtato-v0.1.7...blogtato-v0.1.8) - 2026-03-06

### Added

- add export command

### Other

- split display module ([#61](https://github.com/kantord/blogtato/pull/61))
- split up query.rs ([#60](https://github.com/kantord/blogtato/pull/60))
- create a utils folder ([#59](https://github.com/kantord/blogtato/pull/59))
- remove useless test module ([#58](https://github.com/kantord/blogtato/pull/58))
- add data folder ([#57](https://github.com/kantord/blogtato/pull/57))
- move shorthand logic to separate file ([#56](https://github.com/kantord/blogtato/pull/56))
- create synctato crate ([#55](https://github.com/kantord/blogtato/pull/55))
- create tables module ([#54](https://github.com/kantord/blogtato/pull/54))

## [0.1.7](https://github.com/kantord/blogtato/compare/v0.1.6...v0.1.7) - 2026-03-05

### Added

- add default query ([#50](https://github.com/kantord/blogtato/pull/50))

## [0.1.6](https://github.com/kantord/blogtato/compare/v0.1.5...v0.1.6) - 2026-03-05

### Added

- allow filtering for read and unread posts ([#49](https://github.com/kantord/blogtato/pull/49))
- make syntax more similar to taskwarrior ([#47](https://github.com/kantord/blogtato/pull/47))
- allow marking post unread ([#46](https://github.com/kantord/blogtato/pull/46))

### Fixed

- avoid race condition when running multiple sync at the same time ([#43](https://github.com/kantord/blogtato/pull/43))

### Other

- small refactors ([#48](https://github.com/kantord/blogtato/pull/48))
- reuse RenderCtx ([#45](https://github.com/kantord/blogtato/pull/45))

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
