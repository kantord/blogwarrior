# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/kantord/blogtato/releases/tag/v0.1.0) - 2026-03-01

### Added

- allow reading with cli browsers ([#11](https://github.com/kantord/blogtato/pull/11))

### Fixed

- avoid spamming/blocking when too many false positives
- *(deps)* pin dependencies ([#14](https://github.com/kantord/blogtato/pull/14))
- *(deps)* update rust crate reqwest to v0.13.2 ([#9](https://github.com/kantord/blogtato/pull/9))
- *(deps)* pin rust crate rayon to =1.11.0 ([#7](https://github.com/kantord/blogtato/pull/7))
- *(deps)* update rust crate indicatif to v0.18.4 ([#5](https://github.com/kantord/blogtato/pull/5))
- *(deps)* update rust crate chrono to v0.4.44 ([#4](https://github.com/kantord/blogtato/pull/4))
- fix failing tests
- fix clippy warnings

### Other

- prepare first release ([#17](https://github.com/kantord/blogtato/pull/17))
- add license ([#15](https://github.com/kantord/blogtato/pull/15))
- various refactors ([#13](https://github.com/kantord/blogtato/pull/13))
- clarify some confusing code ([#12](https://github.com/kantord/blogtato/pull/12))
- *(deps)* update rust crate tempfile to v3.26.0 ([#8](https://github.com/kantord/blogtato/pull/8))
- *(deps)* update actions/checkout action to v6 ([#10](https://github.com/kantord/blogtato/pull/10))
- add demo tape ([#6](https://github.com/kantord/blogtato/pull/6))
- ask renovate to make semantic commits
- Update Rust crate httpmock to 0.8
- fetch blogs in parallel
- less fragile remote ref support
- make ci pass
- specify branch in tests
- run ci on mac too
- do not open links while running tests
- Pin dependencies
- add clone command
- use more shards for posts
- simplify adding feeds
- add fancy progress bars
- fold "pull" command into "sync"
- avoid committing lock files
- do not make empty commits
- add some feedback about sync
- only look for git repo on exact path
- add git based sync
- more robust format detection for feeds
- remove redundant check in ci
- make database generic
- add transaction system
- implement store system
- Simplify feed shorthand resolution and standardize error messages
- deduplicate shorthand logic
- use prek for gha checks for consistency
- set up prek
- split things into multiple files
- improve error handling
- avoid data loss when crash happens during write
- remove read command
- handle edge cases
- set up renovate
- add ci
- make error handling more consistent
- use anyhow for error handling
- avoid sneakily overriding data
- add timeout to fetching
- reject incompatible filters
- remove duplicated constants
- simplify cascading deletion logic
- add read command
- add command to read posts
- add shorthands for posts
- more convenient query language
- let user filter by feed
- shorten remove to rm
- show error messages when no matching data was found
- delete feeds by shorthand
- add feed ls command
- do not allow deleting non-existent data
- rename command
- add "feed" prefix for feed commands
- allow deleting items
- remove redundant getter-setters
- add updated_at field
- simplify test code
- avoid confusing field name
- more reasonable estimation for number of posts
- remove needless update method
- avoiding failing when one link is dead
- add fallback id
- add link field
- link post and feed table
- pull metadata for feeds
- customize id length per table
- allow subscribing to feeds
- shard posts into different files
- sorts items based on id in storage file
- store tables as folders
- merge items when pulling
- add hash based id for items
- add pull command
- use jsonlines ormat
- read feeds from input file
- add clap
- add id field for feeds
- normalize url's before using them as an id
- add id field
- improve output formatting
- add different formatting options
- parse dates properly
- separate different feed parsers into different modules
- add some tests
- create common input format
- add extra minimal rss reader example
- initial commit (hello world)
