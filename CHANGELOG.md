# Changelog

## [0.2.7](https://github.com/omar16100/atlassian-cli/compare/v0.2.6...v0.2.7) (2026-04-14)


### Features

* add automated crates.io publishing on release ([5a7de92](https://github.com/omar16100/atlassian-cli/commit/5a7de92bd7cb237c543e77be11423c0f4441f4a6))
* add Bitbucket Bearer auth support and deprecation notices ([f7aecc0](https://github.com/omar16100/atlassian-cli/commit/f7aecc01927e0ed9fa4cb908e354dea89767e393))
* add pipeline variable/secret management commands ([6556d0e](https://github.com/omar16100/atlassian-cli/commit/6556d0e6b1d5fe128ed8fdb9c9749ff2287f4bce))
* pipeline UX fixes — multi-remote detection, --pipeline flag, --wait, --on-complete, --envelope ([f9758fe](https://github.com/omar16100/atlassian-cli/commit/f9758fe2cd3ec2323c6e8bc2b5304522864c45d7))
* SEO overhaul — 24 pages, blog, product pages, runbooks, structured data ([a09217b](https://github.com/omar16100/atlassian-cli/commit/a09217b77b24716b478d4611fc58e33cb7a58ecc))
* watch --timeout, --log mode, steps trigger column, elapsed time, scope-aware 403 hints ([1c9aeba](https://github.com/omar16100/atlassian-cli/commit/1c9aeba9d4e112231a8a2d3f845f86b2b0cb971d))


### Bug Fixes

* add workflow_dispatch trigger to publish-crates workflow ([503ca8d](https://github.com/omar16100/atlassian-cli/commit/503ca8d813e56a21230ab6efc3fb129448b945ab))
* handle Jira ADF description format in issue get/update ([725983e](https://github.com/omar16100/atlassian-cli/commit/725983e63a23dd9773eec0e3f87f57e65c3f84ed))
* migrate Jira bulk search to /search/jql, add HTTP 410 handling ([c699737](https://github.com/omar16100/atlassian-cli/commit/c6997379eba85750537a2764a81e821d09c376ec))
* resolve security audit failures ([c100c58](https://github.com/omar16100/atlassian-cli/commit/c100c58f6da263ba558aaa368e0a26b61dc99afc))
* trigger crates.io publish on tag push instead of release event ([f532400](https://github.com/omar16100/atlassian-cli/commit/f5324004fdd24625201d6028669d06b68353fc4c))
* update aws-lc-rs 1.15.4 → 1.16.2, aws-lc-sys 0.37.0 → 0.39.1 (security advisory) ([a7dd7c5](https://github.com/omar16100/atlassian-cli/commit/a7dd7c5930395978ff47c91dafdbe578ad8ce5d0))
* update landing page to v0.2.9 and add Bearer auth examples ([dede06e](https://github.com/omar16100/atlassian-cli/commit/dede06e971aeba817b89b01773dee086c8ed0864))
* update rustls-webpki 0.103.8 → 0.103.10 (RUSTSEC-2026-0049) ([07a729d](https://github.com/omar16100/atlassian-cli/commit/07a729d6c1f395c4f27b85f61336841eec85fdbe))

## [0.2.6](https://github.com/omar16100/atlassian-cli/compare/v0.2.5...v0.2.6) (2026-01-31)


### Bug Fixes

* handle silent auth failure on empty search results and 403 responses ([1e6946b](https://github.com/omar16100/atlassian-cli/commit/1e6946ba4daefa2d92fc3115d3ca87c45a7b3cc2))

## [0.2.5](https://github.com/omar16100/atlassian-cli/compare/v0.2.4...v0.2.5) (2026-01-27)


### Features

* **website:** add footer credit for Omar Shabab ([ae0e303](https://github.com/omar16100/atlassian-cli/commit/ae0e303a02f3cb63abd5f56f506371980e584f98))


### Bug Fixes

* rename --output to --format and add build number support to logs ([ee629ad](https://github.com/omar16100/atlassian-cli/commit/ee629ad9307d61ce9b61bd1e9507f2c8ac304067))

## [0.2.4](https://github.com/omar16100/atlassian-cli/compare/v0.2.3...v0.2.4) (2026-01-19)


### Bug Fixes

* update whoami to v2.0 and handle Result API change ([48fbf0b](https://github.com/omar16100/atlassian-cli/commit/48fbf0b66a6cb44d3ce968c01b67b9e368cfd58f))

## [0.2.3](https://github.com/omar16100/atlassian-cli/compare/v0.2.2...v0.2.3) (2026-01-19)


### Features

* UX improvements for auth and pipeline commands ([b808571](https://github.com/omar16100/atlassian-cli/commit/b8085717cd10643851e9a9bd4737182624401914))

## [0.2.2](https://github.com/omar16100/atlassian-cli/compare/v0.2.1...v0.2.2) (2026-01-15)


### Features

* add JSM, OpsGenie & Bamboo CLI modules ([0e3014a](https://github.com/omar16100/atlassian-cli/commit/0e3014a804fec2af43b19a6b629e1eb353d5c175))

## [0.2.1](https://github.com/omar16100/atlassian-cli/compare/v0.2.0...v0.2.1) (2025-12-26)


### Features

* comprehensive quality improvements (weeks 1-6) ([f730a29](https://github.com/omar16100/atlassian-cli/commit/f730a29f0520ad26a252c9f41a556a1b75deef79))


### Bug Fixes

* **confluence:** handle draft page publishing with correct version ([ebb2292](https://github.com/omar16100/atlassian-cli/commit/ebb2292c02f8717cce629a23edbae2f8bbe75771))
* remove unnecessary borrows in encryption.rs (clippy) ([9a3838e](https://github.com/omar16100/atlassian-cli/commit/9a3838e398e7800c781a62e7ad1d9ab589ee0b5a))
* remove unused FilterBuilder methods (clippy dead_code) ([1956f22](https://github.com/omar16100/atlassian-cli/commit/1956f22b72aec9826b83ccab7d619a0a31c03cbc))
* update version test for 0.2.0 ([9d8f463](https://github.com/omar16100/atlassian-cli/commit/9d8f4635b983849a94f8b0e68f28d2e7c8e313bc))
* use struct initialization in test (clippy field_reassign_with_default) ([5472727](https://github.com/omar16100/atlassian-cli/commit/5472727fd508b2c33f9e5f71e48ab0ec44ef1884))

## [0.1.9](https://github.com/omar16100/atlassian-cli/compare/v0.1.8...v0.1.9) (2025-12-17)


### Features

* add pipeline enhancements - git context detection, status command, rerun, and variables ([675be8a](https://github.com/omar16100/atlassian-cli/commit/675be8aad4f921e1c7125935ae9fe6090199b1b3))


### Bug Fixes

* apply cargo fmt formatting ([25018a0](https://github.com/omar16100/atlassian-cli/commit/25018a03d57cce058f28e1edb4eaceef4148c76c))

## [0.1.8](https://github.com/omar16100/atlassian-cli/compare/v0.1.7...v0.1.8) (2025-12-15)


### Features

* add landing page website for GitHub Pages ([8c7a92c](https://github.com/omar16100/atlassian-cli/commit/8c7a92c7c89d2c4b168e8f50ce204bfde3eed7b3))


### Bug Fixes

* clippy warnings ([00b9899](https://github.com/omar16100/atlassian-cli/commit/00b9899a6459ec78d0ef4234e4b73c138a828b45))
* update release-please config for cargo workspace ([eb34f6d](https://github.com/omar16100/atlassian-cli/commit/eb34f6d45a4b6c18eb92264b41d05305a39b7f55))
* use simple release type with config files ([21a797e](https://github.com/omar16100/atlassian-cli/commit/21a797e4d0d1b64f6c6d916a8f1177e624578620))
