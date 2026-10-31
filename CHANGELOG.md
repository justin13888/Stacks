# Changelog

## [0.1.1](https://github.com/justin13888/Stacks/compare/v0.1.0...v0.1.1) (2026-10-31)


### Bug Fixes

* drop stale variables from .env.example ([#132](https://github.com/justin13888/Stacks/issues/132)) ([d0f45fa](https://github.com/justin13888/Stacks/commit/d0f45fa4dc59878a811ea0dde7862a56f121aeb0))


### Build System

* keep line tables only in dev builds ([#131](https://github.com/justin13888/Stacks/issues/131)) ([3e361c4](https://github.com/justin13888/Stacks/commit/3e361c4f9a0eb77906f0614e9b0b3b9fabed74f7))


### Continuous Integration

* drop the 0.1.0 release-as pin ([#129](https://github.com/justin13888/Stacks/issues/129)) ([639122a](https://github.com/justin13888/Stacks/commit/639122a2b6519658814ca604c8ecf92116f96072))

## 0.1.0 (2026-10-29)


### Features

* add entry tags and year-in-review stats ([#41](https://github.com/justin13888/Stacks/issues/41)) ([50ef0c7](https://github.com/justin13888/Stacks/commit/50ef0c76a3a8816d3de9c83934b4e606c258aa4f))
* **api:** replace Salvo with kynos and redesign the REST API ([25ac3cc](https://github.com/justin13888/Stacks/commit/25ac3cca8dd37d02d7637d6815e1d42c5da05978))
* **core:** airing-anime and regional drama discovery feeds ([#25](https://github.com/justin13888/Stacks/issues/25)) ([029f55a](https://github.com/justin13888/Stacks/commit/029f55a98e5ebd6a9e4cc7fe9df65abdfa54cf1f))
* **core:** announce new episodes of anilist shows ([#128](https://github.com/justin13888/Stacks/issues/128)) ([bacf916](https://github.com/justin13888/Stacks/commit/bacf916d9e165138b1d1248073362318a6ab5bca))
* **core:** complete an ended show on its last episode ([#113](https://github.com/justin13888/Stacks/issues/113)) ([b8d3858](https://github.com/justin13888/Stacks/commit/b8d3858bfa9af5c3ca3fee7566376b4b7ab4888a))
* **core:** lossless backup round trip and resolvable imports ([8ebeee3](https://github.com/justin13888/Stacks/commit/8ebeee315c573883a4ceecd8302b7736f831a23b))
* **core:** make every metadata call resilient to upstream failures ([f362487](https://github.com/justin13888/Stacks/commit/f362487038437a4237362f95cc7ee73ac32309ff))
* **core:** rebuild the data model on a lossless baseline schema ([7329506](https://github.com/justin13888/Stacks/commit/7329506671035368f85c869ec01e629b9d28a6a0))
* expose title formats, romaji and play ids, and restore plays ([#37](https://github.com/justin13888/Stacks/issues/37)) ([d8fca74](https://github.com/justin13888/Stacks/commit/d8fca74f5d91d741f1e81fbd77df094e26d26af4))
* filter smart lists by category, tag and own score ([#127](https://github.com/justin13888/Stacks/issues/127)) ([98afab1](https://github.com/justin13888/Stacks/commit/98afab18f2933085fb76717b138b86f927f0d5fb))
* let an admin permanently delete a user's account ([#119](https://github.com/justin13888/Stacks/issues/119)) ([6a50359](https://github.com/justin13888/Stacks/commit/6a50359598208625975130275c4570e2e413674e))
* remove titles and date past viewings ([#106](https://github.com/justin13888/Stacks/issues/106)) ([0512823](https://github.com/justin13888/Stacks/commit/0512823eb3a88fe199d680d518c2faf66d42ed70))
* test channels, skip and re-point import rows, filter jobs ([#43](https://github.com/justin13888/Stacks/issues/43)) ([8c4f9d5](https://github.com/justin13888/Stacks/commit/8c4f9d57dfb67636037b31926e1377a119fabc46)), closes [#39](https://github.com/justin13888/Stacks/issues/39)
* **web:** add a smart-list filter editor to the list form ([#97](https://github.com/justin13888/Stacks/issues/97)) ([b54eb67](https://github.com/justin13888/Stacks/commit/b54eb675672d740347cfdb1898d42513f8202bac))
* **web:** design system, app shell, cookie auth, command palette ([15b6bae](https://github.com/justin13888/Stacks/commit/15b6bae5a58565afb722c92a46dfcfb2e5c46106))
* **web:** discover, lists, tags, history with undo and stats ([#36](https://github.com/justin13888/Stacks/issues/36)) ([80905fa](https://github.com/justin13888/Stacks/commit/80905fa67fdd932319c2c8328fcb1a8d51ee9250))
* **web:** settings, import wizard, backup, trash and admin ([#40](https://github.com/justin13888/Stacks/issues/40)) ([98eb725](https://github.com/justin13888/Stacks/commit/98eb725facc52cc2fce9cf0591d068552f8242c1))
* **web:** title detail, library views, up next and calendar ([2a3adb0](https://github.com/justin13888/Stacks/commit/2a3adb0a5ed47ff784df3e6eb729fbed418d829d))


### Bug Fixes

* **api:** honour the configured log level ([#73](https://github.com/justin13888/Stacks/issues/73)) ([a20978c](https://github.com/justin13888/Stacks/commit/a20978c97a22a56b6352b6d9dc58adb589fc036c)), closes [#56](https://github.com/justin13888/Stacks/issues/56)
* **core:** accept only canonical numeric parts in title refs ([#63](https://github.com/justin13888/Stacks/issues/63)) ([07be596](https://github.com/justin13888/Stacks/commit/07be596a5db3eae3e2d2c0d587ea4b8bfe70449b)), closes [#55](https://github.com/justin13888/Stacks/issues/55)
* **core:** acquire the AniList rate slot outside the call deadline ([#31](https://github.com/justin13888/Stacks/issues/31)) ([38dec1c](https://github.com/justin13888/Stacks/commit/38dec1cbe963aa5f911bbcdb1ffbd128367fe654))
* **core:** bound notification deliveries with HTTP timeouts ([05d0792](https://github.com/justin13888/Stacks/commit/05d079275f51fe040b35a2eaafb4b799ecbb2cf0))
* **core:** keep every entry event when restoring a backup ([#99](https://github.com/justin13888/Stacks/issues/99)) ([a66b9e2](https://github.com/justin13888/Stacks/commit/a66b9e27dda1d3e8a5bd8601792fe66a02966999))
* **core:** keep list position keys short as lists grow ([18c1c36](https://github.com/justin13888/Stacks/commit/18c1c3623857ca8825dd926477e5920809d0c81c))
* **core:** prune finished jobs past a configurable retention ([#78](https://github.com/justin13888/Stacks/issues/78)) ([0e94b62](https://github.com/justin13888/Stacks/commit/0e94b624a632596c2cf8ef07924accad36ce879d))
* **core:** refuse a rewatch play before the last run finished ([#120](https://github.com/justin13888/Stacks/issues/120)) ([6f2f34f](https://github.com/justin13888/Stacks/commit/6f2f34f4a1e5401da36482898f3f8baab718c422))
* **core:** refuse GCP's IPv6 metadata address ([#118](https://github.com/justin13888/Stacks/issues/118)) ([1d255a2](https://github.com/justin13888/Stacks/commit/1d255a2fcd5b0ec21b53aaff0c102a98bef39e3b))
* harden compose for SELinux hosts and pass every WW_ variable ([#96](https://github.com/justin13888/Stacks/issues/96)) ([87c85f7](https://github.com/justin13888/Stacks/commit/87c85f7b47e2d777374d6ac3224ac921302619c3))
* harden web headers, split-origin CSRF and CSV export ([#105](https://github.com/justin13888/Stacks/issues/105)) ([255f04d](https://github.com/justin13888/Stacks/commit/255f04d975b7033f76c01cf69a54e7f8078e6f43))
* install the metrics recorder first and fix the breaker test ([#100](https://github.com/justin13888/Stacks/issues/100)) ([7ffae93](https://github.com/justin13888/Stacks/commit/7ffae93512b2dffa5a14fd8c5c65c85fa08571f0))
* keep credentials out of logs and classify a bad TMDB key ([#111](https://github.com/justin13888/Stacks/issues/111)) ([efbce2c](https://github.com/justin13888/Stacks/commit/efbce2c4f604268301ae2224b4c49e614a7b59cd))
* log episodes for AniList shows ([#107](https://github.com/justin13888/Stacks/issues/107)) ([033f8ac](https://github.com/justin13888/Stacks/commit/033f8ac7ee0b9e449fba3f9e29d5d534da68bf60))
* never overwrite a library entry when adding a title ([#101](https://github.com/justin13888/Stacks/issues/101)) ([ecafabc](https://github.com/justin13888/Stacks/commit/ecafabc444b9988830e2f8e3ead5ac2c9fee69a8))
* refuse a play dated before the previous run's completion ([#114](https://github.com/justin13888/Stacks/issues/114)) ([a8d803e](https://github.com/justin13888/Stacks/commit/a8d803e7a312c9426915ce69cbcd45206975e78d))
* refuse internal addresses for notification deliveries ([#116](https://github.com/justin13888/Stacks/issues/116)) ([71d5bc7](https://github.com/justin13888/Stacks/commit/71d5bc7139f5c15ea22e1196e57bed4d8bae16cc))
* release in-flight jobs when the api shuts down ([#69](https://github.com/justin13888/Stacks/issues/69)) ([8c2dde6](https://github.com/justin13888/Stacks/commit/8c2dde6d1a5e2a6f113588e1e41c27454caed083)), closes [#47](https://github.com/justin13888/Stacks/issues/47)
* **web:** let only the latest notes save change the save status ([#35](https://github.com/justin13888/Stacks/issues/35)) ([e066695](https://github.com/justin13888/Stacks/commit/e066695bc188f1d9a58c7360b4a31d8bc21cceeb))
* **web:** send notes saves in order so an older note never wins ([#72](https://github.com/justin13888/Stacks/issues/72)) ([6dd22d2](https://github.com/justin13888/Stacks/commit/6dd22d2e68faee8fc6a115d8e035b5b894c9bde3))


### Performance Improvements

* **core:** batch the episode upserts of a season read ([#125](https://github.com/justin13888/Stacks/issues/125)) ([b9e04d4](https://github.com/justin13888/Stacks/commit/b9e04d421d13fb605aac96dcb3b1e4c9a73b59c8))


### Code Refactoring

* **api:** drop the unused /data volume and stacks_data mount ([46a6dd3](https://github.com/justin13888/Stacks/commit/46a6dd352cc866f7c0fd0e431689c6d04099ae6a))
* **core:** remove dead code and tidy module boundaries ([#77](https://github.com/justin13888/Stacks/issues/77)) ([3ba91a7](https://github.com/justin13888/Stacks/commit/3ba91a701d3a3ca2358707707d2543bf0cbe89c6))
* **core:** remove unused WW_DATA_DIR and WW_CAMEO_CACHE_DIR ([#62](https://github.com/justin13888/Stacks/issues/62)) ([29feeb9](https://github.com/justin13888/Stacks/commit/29feeb99b0e0644478c93fa4e7d6b599da3d4f1f))
* remove the never-raised upcoming_release event ([#79](https://github.com/justin13888/Stacks/issues/79)) ([ae72845](https://github.com/justin13888/Stacks/commit/ae72845b8c496b13f1219393812f66c5fcf35ab9))
* **web:** move shared helpers to ui and lib, fix test timing ([#59](https://github.com/justin13888/Stacks/issues/59)) ([033981b](https://github.com/justin13888/Stacks/commit/033981be0081b1f086a09cd0d9965495b1b554dc))


### Documentation

* align contributor docs and ADRs with the kynos codebase ([#76](https://github.com/justin13888/Stacks/issues/76)) ([406abb1](https://github.com/justin13888/Stacks/commit/406abb128ddbcdd087c729fed17d80477c3f7e61))
* bring the docs in line with the shipped product ([#108](https://github.com/justin13888/Stacks/issues/108)) ([2f99013](https://github.com/justin13888/Stacks/commit/2f9901328b9b1a840cb009814e33feacd92d2982))
* drop stale WW_SECRET_KEY and gen-secret setup instructions ([#24](https://github.com/justin13888/Stacks/issues/24)) ([9ed4c52](https://github.com/justin13888/Stacks/commit/9ed4c522928d8b24171b5aad32915e3fca96ac6a))
* state ADR rules in the present tense ([#70](https://github.com/justin13888/Stacks/issues/70)) ([64f074f](https://github.com/justin13888/Stacks/commit/64f074f3974fb2f9714c107b72aa6bd60c2382c0))


### Build System

* **deps:** update vulnerable deps, pin images, add dependabot ([#98](https://github.com/justin13888/Stacks/issues/98)) ([f4e0747](https://github.com/justin13888/Stacks/commit/f4e0747dc735b98733bfab7dec4cbb43846fe2cd))


### Continuous Integration

* **deps:** bump the actions group with 6 updates ([#104](https://github.com/justin13888/Stacks/issues/104)) ([ba71cee](https://github.com/justin13888/Stacks/commit/ba71cee7cf002f585c6bafa018294e1e996ecfc8))
* enforce conventional commits and automate signed, attested releases ([#15](https://github.com/justin13888/Stacks/issues/15)) ([5c2ab9e](https://github.com/justin13888/Stacks/commit/5c2ab9e4bbca0ebeff2b5fd78bd19fe94be843d0))
* gate coverage at 80%, file length and mutants; fix CI gaps ([#60](https://github.com/justin13888/Stacks/issues/60)) ([02a658c](https://github.com/justin13888/Stacks/commit/02a658c142f9e7e89cd3aff68dce7d8e5d8b1588))
* lint every master push and document title-only squashes ([#95](https://github.com/justin13888/Stacks/issues/95)) ([aa863a3](https://github.com/justin13888/Stacks/commit/aa863a3c9290f47cc89e7d91a4a5a93baa994e88))
