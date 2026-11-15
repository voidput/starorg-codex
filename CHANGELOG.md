# Changelog

## [0.0.2](https://github.com/voidput/starorg-strings/compare/v0.0.1...v0.0.2) (2026-11-15)


### ⚠ BREAKING CHANGES

* rename the app to StarOrg Codex

### Features

* **app:** Developer mode contract, dev-worker client and mock ([b3ca32d](https://github.com/voidput/starorg-strings/commit/b3ca32dc3eb70fcbf9faee4eb61765bfb0ce14c5)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Explorer, Datasets and Diff screens behind Developer mode ([92f4de8](https://github.com/voidput/starorg-strings/commit/92f4de80699df0a326933360f7e50caa82eb30d0)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Privacy button in the footer with what leaves the computer ([9262829](https://github.com/voidput/starorg-strings/commit/9262829cd02137e87ffc3af21ce695d7fbae5c2d)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Settings -&gt; Experimental -&gt; Local MCP server ([3ed8771](https://github.com/voidput/starorg-strings/commit/3ed87710a1f1a14d6d3775c4a846f38d415782b1)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **cli:** dev record, export-type and types ([544bba0](https://github.com/voidput/starorg-strings/commit/544bba01fab337327e16ff2108f9fde7e86a0e20)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **core:** explorer index, record replies, dataset shapes and diff for Developer mode ([e5dac63](https://github.com/voidput/starorg-strings/commit/e5dac63e272703be81c9d4beb9692a1e53a89863)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **data:** dev-worker protocol, explorer database and streamed exports ([dccd11a](https://github.com/voidput/starorg-strings/commit/dccd11a5675fdd54a2eda768d4109906471887f8)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **gamedata:** port sc-gamedata's query layer (fuzzy, obtain, where, shops) ([15e0beb](https://github.com/voidput/starorg-strings/commit/15e0beb6612e3ac4903ea2b54f9241d27d1a53ef)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **mcp:** experimental local MCP server, stdio in the CLI ([eb7f3b6](https://github.com/voidput/starorg-strings/commit/eb7f3b60a10bc338c9cf02cb7b78c4c70d828e94)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* rename the app to StarOrg Codex ([2155578](https://github.com/voidput/starorg-strings/commit/2155578d6a24c6fbab235541a94a9137859320f1)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)


### Bug Fixes

* **app:** bring back the Strings generator filter beside the group tree ([fcbbe46](https://github.com/voidput/starorg-strings/commit/fcbbe46e8f8924cc7c2bb94f1844fa3da2025074)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Privacy -&gt; Network settings opens Settings at the Network card ([224b8f7](https://github.com/voidput/starorg-strings/commit/224b8f76d9e81a97452ee119c5e4c7ef7a397e55)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Settings section links no longer shift the whole window ([56c7f74](https://github.com/voidput/starorg-strings/commit/56c7f74d308d929fbcc2ee69fa43a390595f0089)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **dev:** render ragged dataset rows without dropping columns ([2a83739](https://github.com/voidput/starorg-strings/commit/2a8373902ee8a3fcc071e435533dc46da08ca1e6)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **mcp:** mark inexact name matches; resolve ship weapons and components ([b861bc3](https://github.com/voidput/starorg-strings/commit/b861bc3bd010b604743de0ce43d62bb25c2054f2)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)


### Performance Improvements

* **gamedata:** prune blueprint item records; count loot pools once per report ([9ecfaf4](https://github.com/voidput/starorg-strings/commit/9ecfaf4b391dae96076f002a1abce4a9ae432607)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **gamedata:** prune item records as they load in the three item-wide domains ([08bd33a](https://github.com/voidput/starorg-strings/commit/08bd33a160b6a7ab5ecec758d54fa60206f65ca3)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)

## 0.0.1 (2026-11-15)


### Features

* **app:** all v1 screens on the mock backend (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([e066e3a](https://github.com/voidput/starorg-strings/commit/e066e3ab22e6b46953ddd0d0b52fe00e8b7612df))
* **app:** app frame and first-run setup wizard on the mock backend (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([5431ab9](https://github.com/voidput/starorg-strings/commit/5431ab951176805a727c73cce77689f95b92c299))
* **app:** check GitHub releases for updates; signed in-app install (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([63e7359](https://github.com/voidput/starorg-strings/commit/63e7359bb57110b9087148c6bdc3bbe1b1b21c7e))
* **app:** contract follows M1/M2: apply outcomes, remove allow_foreign, M2 profile, rule names, user patches (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([4fcba22](https://github.com/voidput/starorg-strings/commit/4fcba22dd05bd17ba611b6fa12b826fc38f57e1b))
* **app:** contract for patch detail, StarOrg pull/push, diagnostics, last apply (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([d11fa0e](https://github.com/voidput/starorg-strings/commit/d11fa0e4e3c3fb82354ec49f65a39ea0ff072075))
* **app:** contract gains save_diagnostics and not-implemented; presets and schema shared (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([dbab357](https://github.com/voidput/starorg-strings/commit/dbab357dd995c1b4bdb8241ae9261bd91c506b0b))
* **app:** fleet and ship-names contract, real backend and mocks (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([471b52d](https://github.com/voidput/starorg-strings/commit/471b52dec8ef309aa2531368c033092721251bc2))
* **app:** Microsoft Store flavour and Windows apply tests on elevated runners (Refs [#16](https://github.com/voidput/starorg-strings/issues/16)) ([22a87e7](https://github.com/voidput/starorg-strings/commit/22a87e7af7dd61e1674cca6bc1ed72e59d230454))
* **app:** real backend over starorg-core is the default (Refs [#15](https://github.com/voidput/starorg-strings/issues/15), Refs [#16](https://github.com/voidput/starorg-strings/issues/16)) ([11d3582](https://github.com/voidput/starorg-strings/commit/11d3582098992217b70ce6b8f39222feaf5242a7))
* **app:** Ships screen - import, order, groups and renames (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([70269d2](https://github.com/voidput/starorg-strings/commit/70269d2f99542ac32dff5a643dcf9fb953510a77))
* **app:** Strings group tree, group-by, chips and keyboard ([474fc12](https://github.com/voidput/starorg-strings/commit/474fc126b0eff13ea34c8ae91852f72c79330e43)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Tauri 2 + Svelte 5 app shell with StarOrg/RSI+ tokens ([f27b8cd](https://github.com/voidput/starorg-strings/commit/f27b8cd742e3d6b08b2f3c24ef85563ee967a510)), closes [#8](https://github.com/voidput/starorg-strings/issues/8)
* **app:** typed command contract with mock backend (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([9b4d110](https://github.com/voidput/starorg-strings/commit/9b4d110ae71742bdb0fec793e949b97fb40813c8))
* **app:** update banner and Check for updates settings (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([b479c75](https://github.com/voidput/starorg-strings/commit/b479c753e096b1fa23530ec6a327ffe61fdf6ab5))
* **app:** Windows wording in the setup wizard; Store settings render (Refs [#9](https://github.com/voidput/starorg-strings/issues/9)) ([1d12954](https://github.com/voidput/starorg-strings/commit/1d1295441317e27a5ecc9e8a2c55e67847ffa9ab))
* **cli:** build applies the enhancement profile and writes coverage ([11975c9](https://github.com/voidput/starorg-strings/commit/11975c9141446842417e1d299b2c79ebfc24561a)), closes [#13](https://github.com/voidput/starorg-strings/issues/13) [#14](https://github.com/voidput/starorg-strings/issues/14)
* **cli:** channels and build commands ([2e797dc](https://github.com/voidput/starorg-strings/commit/2e797dc09025ecd4fada03a8aff4f4fc3dca5306)), closes [#4](https://github.com/voidput/starorg-strings/issues/4) [#5](https://github.com/voidput/starorg-strings/issues/5)
* **cli:** fleet import, list, match and clear (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([9424d6e](https://github.com/voidput/starorg-strings/commit/9424d6e025dee590e80a93209f9ff65c06f0c4d3))
* **cli:** hook prelaunch|postexit|install|uninstall|status and blueprints commands ([f335f4c](https://github.com/voidput/starorg-strings/commit/f335f4c482a5f80cfb81cbbb51848b24a82cf1a6)), closes [#11](https://github.com/voidput/starorg-strings/issues/11) [#17](https://github.com/voidput/starorg-strings/issues/17)
* **cli:** setup/apply/status/remove/restore/backups/patches/doctor ([5fad7c0](https://github.com/voidput/starorg-strings/commit/5fad7c0e016be419f9ba0eef43fc30ab11a70f92)), closes [#10](https://github.com/voidput/starorg-strings/issues/10)
* **cli:** version prints version and build details, no network (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([99167d2](https://github.com/voidput/starorg-strings/commit/99167d2ed8ef5040b1694bb3707e0af2dafd8114))
* **contract:** group fields, group tree and read_string_groups ([8a90573](https://github.com/voidput/starorg-strings/commit/8a90573589fee08a694f8c5a64aed35f1634ff52)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **core,cli:** GameData names for the import matcher, owned store for [Owned] on apply ([3671a99](https://github.com/voidput/starorg-strings/commit/3671a99a341f833943befea503b5bd5305e9784b))
* **core,data:** strings grouping rules and the loc-reference pass ([7c4b3dd](https://github.com/voidput/starorg-strings/commit/7c4b3dd7cd1c9c615d5fc26dc58ff6c185bd92eb)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **core,net,cli:** published dumps source, gamedata command, Wine-prefix output guard ([be75b2c](https://github.com/voidput/starorg-strings/commit/be75b2caab864fa6f4d6b458e8b6bb2d06fdcb83)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **core:** apply core (spec 7): journal, locks, guards, validation, backups ([2349934](https://github.com/voidput/starorg-strings/commit/2349934ad2e945754ee3268b2c0a2bedaf788aed)), closes [#10](https://github.com/voidput/starorg-strings/issues/10)
* **core:** data patch entries and the built-in Hockrow patches ([9acbce5](https://github.com/voidput/starorg-strings/commit/9acbce562122247f8370eade8fc4a1dfdf5b48bc)), closes [#14](https://github.com/voidput/starorg-strings/issues/14)
* **core:** enhancement generators for stats, name tags and missions ([c7c0c4c](https://github.com/voidput/starorg-strings/commit/c7c0c4ceff4b74f62db1afaccc54fe3d5967dc87)), closes [#13](https://github.com/voidput/starorg-strings/issues/13) [#14](https://github.com/voidput/starorg-strings/issues/14)
* **core:** fleet import, hull catalogue and ship-names generator (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([35afa19](https://github.com/voidput/starorg-strings/commit/35afa194d6318e7ab2dae2b104337ae31c48894b))
* **core:** fleet listing with final names and override flags (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([6cff567](https://github.com/voidput/starorg-strings/commit/6cff567b844baa7c48ce290a518fb310615d67ab))
* **core:** GameSource trait, build identity, install discovery, resolve_ci ([cd68850](https://github.com/voidput/starorg-strings/commit/cd6885053f6e7c7abdbde2663e9d90b10c6533bb)), closes [#5](https://github.com/voidput/starorg-strings/issues/5)
* **core:** global.ini reader/writer with byte-identical round trip ([3d2f566](https://github.com/voidput/starorg-strings/commit/3d2f56629881120393ae6117bf41a42e14af4bc6)), closes [#4](https://github.com/voidput/starorg-strings/issues/4)
* **core:** native Windows install readiness (Refs [#9](https://github.com/voidput/starorg-strings/issues/9), [#10](https://github.com/voidput/starorg-strings/issues/10)) ([222e273](https://github.com/voidput/starorg-strings/commit/222e2730710e876779c6c58bea1ce3441c3cf403))
* **core:** per-character owned blueprints, log scan, Smart Citizen import, hook blocks ([7372bde](https://github.com/voidput/starorg-strings/commit/7372bde9c19051266c4235dd65af419726e004e2)), closes [#11](https://github.com/voidput/starorg-strings/issues/11) [#17](https://github.com/voidput/starorg-strings/issues/17)
* **data:** extraction worker with entry-count check (eocd.rs) ([a274aa3](https://github.com/voidput/starorg-strings/commit/a274aa3882ebf7d1fe5954bbcc4f4af5c186d289)), closes [#3](https://github.com/voidput/starorg-strings/issues/3)
* **data:** GameData model and local extraction from DataCore ([559ce65](https://github.com/voidput/starorg-strings/commit/559ce656346e5a8e6af0c264f5dab573a7ed7187)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** item helpers (item-common, tag index, Math.round) for the item domains ([46d1c4e](https://github.com/voidput/starorg-strings/commit/46d1c4ea2a39063e99d0a1c8fe3cc9c7ca1faf20)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port ammo, byte-identical on LIVE 4.10.200 ([40b3ab6](https://github.com/voidput/starorg-strings/commit/40b3ab6608be8cca8d569de596279b05f0e7eef7)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port armor, byte-identical on LIVE 4.10.200 ([10b4e68](https://github.com/voidput/starorg-strings/commit/10b4e68a753c1abdda69f9caed2897c110274fc1)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port blueprints, byte-identical on LIVE ([6299551](https://github.com/voidput/starorg-strings/commit/629955195ab65bc4a29ee7fd799c6f9ddf28f637)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port cargo, byte-identical on LIVE ([59a9c4d](https://github.com/voidput/starorg-strings/commit/59a9c4d0081aec53f92a0d2f6df474f0e65a4c3e)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port commodities domain, byte-identical on LIVE ([0550389](https://github.com/voidput/starorg-strings/commit/0550389f8879bb07832a1d86f1465b3038c87d1f)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port consumables, byte-identical on LIVE 4.10.200 ([6ac3024](https://github.com/voidput/starorg-strings/commit/6ac3024029a788c39ddc8811358fe2eefc1e8577)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port fps-weapons; NBSP-aware localeCompare ([8f18f23](https://github.com/voidput/starorg-strings/commit/8f18f235f7b4bd557436388f719689c6b3a0a712)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port game-search-index and report golden-only datasets ([9586653](https://github.com/voidput/starorg-strings/commit/9586653e9d1d9cfcae30bf0d336839074eec9de9)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port item-sources, byte-identical on LIVE 4.10.200 ([b7f7a4c](https://github.com/voidput/starorg-strings/commit/b7f7a4c182480e1bd92bb09eec37f023f5b053b3)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port item-tags ([f2aaa8d](https://github.com/voidput/starorg-strings/commit/f2aaa8d41b6448275fd92a7bee51bc0a70e1aeba)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port law, byte-identical on LIVE ([095d536](https://github.com/voidput/starorg-strings/commit/095d53670aa6e8efef21310150a9a7a9949814e5)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port loot, byte-identical on LIVE ([44df110](https://github.com/voidput/starorg-strings/commit/44df110cd8208238e339892f6a2f07a01d4ab21f)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port mining domain, byte-identical on LIVE ([ffd2b4e](https://github.com/voidput/starorg-strings/commit/ffd2b4e957d652c776bd220b07a2f013ce234ace)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port mining-spawns domain, byte-identical on LIVE ([cff8318](https://github.com/voidput/starorg-strings/commit/cff831833a5966838c8b55072c41831b65904bfa)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port missions, byte-identical on LIVE ([b2c545b](https://github.com/voidput/starorg-strings/commit/b2c545b77b7dcdbb4b22a5cc16edce1b1c6adcfd)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port quantum and containers, byte-identical on LIVE ([2570fe6](https://github.com/voidput/starorg-strings/commit/2570fe6c66ae61ab946ba43dbacbc2f324870302)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port refining domain, byte-identical on LIVE ([579d8de](https://github.com/voidput/starorg-strings/commit/579d8de175ef9ded24367b884a3252254ac141a2)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port reputation, byte-identical on LIVE ([36cb8ba](https://github.com/voidput/starorg-strings/commit/36cb8ba4d28ada7d543f9ed6e1544fcc516c3c8a)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port salvage domain, identical on LIVE ([0b70784](https://github.com/voidput/starorg-strings/commit/0b70784d7767d59a2b63334f4bdf85f77ed21857)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port ship-components, byte-identical on LIVE ([4729cd2](https://github.com/voidput/starorg-strings/commit/4729cd26febf744d9c87fb1b62405ebcb4587717)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port ships; every vehicle identical on LIVE ([7734cc2](https://github.com/voidput/starorg-strings/commit/7734cc201b46be4f970f5ab40fa981ace0b6a056)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port shops domain, byte-identical on LIVE ([bf575d0](https://github.com/voidput/starorg-strings/commit/bf575d0eaed54d5882cd42020fe220d599422f49)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port starmap domain, byte-identical on LIVE ([b9722ee](https://github.com/voidput/starorg-strings/commit/b9722eee4d7aa39ab35c2327ee84df6157e03002)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port the overlay pack registry and loading ([c1038ea](https://github.com/voidput/starorg-strings/commit/c1038eaadde01a24de5493c269b1bfc844809361)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port the socpak object-container helper ([905fad1](https://github.com/voidput/starorg-strings/commit/905fad18477caf490b63d0c5fc2ded43b849d201)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port weapons, byte-identical on LIVE 4.10.200 ([ddd5e2d](https://github.com/voidput/starorg-strings/commit/ddd5e2d217486f955c566d45995ec24c470e085e)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** port wikelo trades, byte-identical on LIVE ([0e2a409](https://github.com/voidput/starorg-strings/commit/0e2a4098807b2ce02b524913a802c56b75429a4b)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **gamedata:** scaffold the Rust port of sc-gamedata, manufacturers byte-identical ([d3c6c4f](https://github.com/voidput/starorg-strings/commit/d3c6c4fca10b77d9f76f9e9ae46b45833b9ccff1)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **net:** release list request for the app's update check (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([04a5333](https://github.com/voidput/starorg-strings/commit/04a5333ebcb1e9eeed926f8312e2f6e19e5b1445))


### Bug Fixes

* **app:** embed common-controls manifest for all Windows targets so tests run (Refs [#6](https://github.com/voidput/starorg-strings/issues/6)) ([99d135a](https://github.com/voidput/starorg-strings/commit/99d135adc891876e512684a60b2ea6a5bdb8d87a))
* **app:** import dialog fits the window; long name lists scroll (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([045d379](https://github.com/voidput/starorg-strings/commit/045d3796098794de44e383afb9a3666c7676dbd4))
* **app:** in-game preview no longer clipped; wider preview column on large windows (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([35ca53d](https://github.com/voidput/starorg-strings/commit/35ca53da5726ba90f38b57d91601e3ba7b0063b8))
* **app:** review fixes for outcomes, unsaved edits, jobs, dialogs and a11y (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([01e25c7](https://github.com/voidput/starorg-strings/commit/01e25c7da0e145d94786990457500fd9fbfcb9d3))
* **app:** setup again keeps profiles; unbuilt network switches off (Refs [#15](https://github.com/voidput/starorg-strings/issues/15)) ([4c86bf1](https://github.com/voidput/starorg-strings/commit/4c86bf1d7cfd9aa7c80871b35084714d4203ad8f))
* **app:** update switch waits for the settings save before checking ([589baee](https://github.com/voidput/starorg-strings/commit/589baeec3d4593c84ed01cb4ab7cae20d5a02e1a)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **core,cli:** no stale user.cfg write on a shared file; keep one-off picks and links ([a348152](https://github.com/voidput/starorg-strings/commit/a348152a9fc1d7cb382e16d7bcd120925c56810a))
* **core,cli:** normalise dangling USER.cfg links; kebab remove action; quiet exit on EPIPE (Refs [#10](https://github.com/voidput/starorg-strings/issues/10)) ([a8dd253](https://github.com/voidput/starorg-strings/commit/a8dd25325cc1bb4abc94d1dc73b82a056a0cff46))
* **core:** ambiguous owned names mark nothing ([1b898a2](https://github.com/voidput/starorg-strings/commit/1b898a2227ba2dd4c53fffb46bc6c2a345263eb9))
* **core:** hook block round trips without a final newline, keeps continued exits whole ([b8992f0](https://github.com/voidput/starorg-strings/commit/b8992f0e1157ad34d6e4ef53acef26ccb75efea8)), closes [#11](https://github.com/voidput/starorg-strings/issues/11)
* **core:** one validator for build and apply; BOM rule unconditional; no -1 sentinels ([766f9a4](https://github.com/voidput/starorg-strings/commit/766f9a4da0575ab41022d161b189e744ead78556))
* **core:** retry renames on Windows when a just-written file is held open (Refs [#10](https://github.com/voidput/starorg-strings/issues/10)) ([1c5f81f](https://github.com/voidput/starorg-strings/commit/1c5f81fba061e237d561f240aca47de4c4a8705d))
* **fleet:** verifier findings - CSV rows never stored as names, Wikelo hulls, short keys ([5e17db4](https://github.com/voidput/starorg-strings/commit/5e17db4165caa34af8a6ab3275fc6d616ca7308c)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **gamedata:** make the golden compare a byte compare ([2a7174d](https://github.com/voidput/starorg-strings/commit/2a7174d90d42125528e4af7d1fe788c6d8aadec8)), closes [#12](https://github.com/voidput/starorg-strings/issues/12)
* **groups:** prefixes beat entities that only display a key; facets only where they mean something ([149de5c](https://github.com/voidput/starorg-strings/commit/149de5cf615c27e0f5cf436b905e446e9ca5093d)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **release:** bump app crate lock entry and app package files with the workspace ([5d0c9ab](https://github.com/voidput/starorg-strings/commit/5d0c9ab78722ce90c4dd2924e4a93a16e9f8a99d)), closes [#7](https://github.com/voidput/starorg-strings/issues/7)
* **release:** bump starorg-net in Cargo.lock on release; CI checks every crate is covered ([f6d5de4](https://github.com/voidput/starorg-strings/commit/f6d5de4b5e6206fa1022a5778c56cbc7765552b1))
* **store:** no MSIX for 0.x versions, Store releases start at 1.0.0 (Refs [#16](https://github.com/voidput/starorg-strings/issues/16)) ([72efffb](https://github.com/voidput/starorg-strings/commit/72efffb899fef5d272a19d8d9c9b591508123ac1))
* **windows:** verify signatures, locate signtool, shorter access-denied retries (Refs [#16](https://github.com/voidput/starorg-strings/issues/16)) ([937b48c](https://github.com/voidput/starorg-strings/commit/937b48cb642658155988474e031ffb44e92687be))
