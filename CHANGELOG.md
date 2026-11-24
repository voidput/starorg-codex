# Changelog

## [1.6.0](https://github.com/voidput/starorg-codex/compare/v1.5.0...v1.6.0) (2026-11-24)


### Features

* **dev:** Diff offers each channel's older cached builds as read-only sources ([8452630](https://github.com/voidput/starorg-codex/commit/84526300f9dc5ab11aaeea057dd4c089120db9be))
* **shops:** item search: every shop that sells or buys an item ([bb0c26c](https://github.com/voidput/starorg-codex/commit/bb0c26c0d8926c419f08582e1b4126d20072409e))
* **telemetry:** alerts over live frames behind their own Notifications choice ([e7dd314](https://github.com/voidput/starorg-codex/commit/e7dd3144eb90d2f892fdc1e3c12c43e7ccfb14d4))
* **telemetry:** experimental live telemetry crate (loopback gRPC, run-time schema) ([40f0e81](https://github.com/voidput/starorg-codex/commit/40f0e8169caf8289c78d27dbae6f8e03707baea0))
* **telemetry:** Game.log events and a local People panel (experimental, off) ([9a6372a](https://github.com/voidput/starorg-codex/commit/9a6372a82be743652a34947d11e2032d33328b16))
* **telemetry:** history graphs, session recording and replay ([17a9d04](https://github.com/voidput/starorg-codex/commit/17a9d0474b59e1c801e37614a8844e44e4d5a8ea))
* **telemetry:** live telemetry screen, settings section and Tauri backend (experimental, opt-in) ([dfb10e6](https://github.com/voidput/starorg-codex/commit/dfb10e68e2a64d50e6886d228bb41319c20c0af4))
* **telemetry:** one choice in Settings: Off, Metrics or Metrics + actions; notifications get their own switch later ([aa79947](https://github.com/voidput/starorg-codex/commit/aa79947f0e11b56ce16e41ca68be6b4097ae589c))
* **telemetry:** opt-in gate for live telemetry, managed USER.cfg lines and launcher env help ([732113e](https://github.com/voidput/starorg-codex/commit/732113e18d2b2cb768f3f8c6db4b75f9e7494a0c))
* **telemetry:** shard join history, and a coherent phase 2 screen and settings ([a970aca](https://github.com/voidput/starorg-codex/commit/a970aca4c4fb1ffd4f790cf5741f28f25d913f34))


### Bug Fixes

* **app:** say restart when the app file was replaced while open, instead of a bare worker start error ([cf1e76f](https://github.com/voidput/starorg-codex/commit/cf1e76f8b06f10029fa74b611073704864511d2a))
* **dev:** cached-build Diff compares shared datasets only, shared reading locks beside the folder, stale picks re-picked ([15d2b40](https://github.com/voidput/starorg-codex/commit/15d2b405fda9e5ee77059c1bbc5e522982a6026a))
* **hooks:** bound launch hooks to their timeout and run all work at low priority ([8292c95](https://github.com/voidput/starorg-codex/commit/8292c95baf975f055c03c8c29cc44ff47c727f45))
* launch hooks stay in their time limit and run at low priority; live telemetry phase 2, shops item search ([d4fc2a7](https://github.com/voidput/starorg-codex/commit/d4fc2a7e91946104e0a790a9bb64f76f88563547))
* **mcp:** Windows: release a reading lock before deleting it, open folders for mtime in tests with backup semantics ([6df1e9f](https://github.com/voidput/starorg-codex/commit/6df1e9f45d75a62ce576814adf011ae3429a13f5))
* **shops:** item search merges a shop's rows of one item, names only match names ([9f9808a](https://github.com/voidput/starorg-codex/commit/9f9808a390abbb41330f645c603c0aefaf8624b7))
* **telemetry:** one channel carries the USER.cfg lines, config only after USER.cfg, review fixes ([8708cc8](https://github.com/voidput/starorg-codex/commit/8708cc8bdb6e070c90b0667f9d6f03c74702ca3c))
* **telemetry:** shard history gets its own switch and a Clear that stays cleared ([bdabe70](https://github.com/voidput/starorg-codex/commit/bdabe706f9ac25c9cf8ec6270e4778b186df6840))

## [1.5.0](https://github.com/voidput/starorg-codex/compare/v1.4.0...v1.5.0) (2026-11-23)


### Features

* **app:** Backups and Activity move to an App group with Settings ([8d3334b](https://github.com/voidput/starorg-codex/commit/8d3334b4408dad5fffb5d8a5426c7b119d1aec82))
* **app:** Commodities tab with lazy list and per-resource detail ([e5c3f1f](https://github.com/voidput/starorg-codex/commit/e5c3f1f1c3243ddf7b4e857110e82e2b874b64c1))
* **app:** Fix missing item names switch, counts, preview and docs ([f3d5a24](https://github.com/voidput/starorg-codex/commit/f3d5a241ff54936230658c01d16eba506760b4b4))
* **app:** link ship items on the Shops screen after Read game data ([b20e362](https://github.com/voidput/starorg-codex/commit/b20e362207eaf6ec408c07e5a721d59b0367fc78))
* **app:** Loot screen with what drops where ([c98aa8b](https://github.com/voidput/starorg-codex/commit/c98aa8b304abb59580a02645801851a3e29ba070))
* **app:** preview shows the open section's examples and can read game data ([2ee7c65](https://github.com/voidput/starorg-codex/commit/2ee7c65f7d8aa70af726e42a6330e1b842311cdc))
* **app:** read_game_data command and preview examples per section ([a68df95](https://github.com/voidput/starorg-codex/commit/a68df95d62e6207aa840e78a57cac08af28a37c1))
* **app:** Shops screen listing what each shop sells ([95f02ac](https://github.com/voidput/starorg-codex/commit/95f02ac10fe77785d7d5849432284f2852bac4c6))
* **app:** UEX token in Settings and Fetch now for UEX market data ([cd20aab](https://github.com/voidput/starorg-codex/commit/cd20aabc472cef34f1da2e5bf1bfb171ea921d51))
* **app:** Where to get tab, obtain preview samples, UEX status, counts and docs ([622fbfd](https://github.com/voidput/starorg-codex/commit/622fbfd6a0b9dc8c0e8bead1ec472fd1a8b81fdd))
* Commodities and Loot, trade calculator, WHERE TO GET, Shops, missing names, disk limits, faster loading ([304418e](https://github.com/voidput/starorg-codex/commit/304418e017210cd56a3ffb15d1192a6c07c7de6c))
* **commodities:** buy/sell trade calculator in the commodity detail ([ffd8add](https://github.com/voidput/starorg-codex/commit/ffd8addd175922596cab96fd7c01c2df1f54fa33))
* **enhance:** fix missing item names and guard generated text against raw keys ([c5e7c3d](https://github.com/voidput/starorg-codex/commit/c5e7c3d4c933a23e3f03f4240d5496e2439337e9))
* **enhance:** name missing variants after their base item ([19b9bc6](https://github.com/voidput/starorg-codex/commit/19b9bc6793d3564b79aa01095ab42d23e95eb0c7))
* **enhance:** obtain hints core: WHERE TO GET generator, engine extend, profile modes ([349932b](https://github.com/voidput/starorg-codex/commit/349932b726551162c69093bf8d2f9a4acb56f422))
* **enhance:** obtain hints data: desc keys pass, bulk obtain filler, apply/CLI/app wiring ([5dd400a](https://github.com/voidput/starorg-codex/commit/5dd400a817dee6aed0c5d50d4d03eeb2c22a4535))
* **enhance:** one name chain for items; curated names feed variants ([565b3cd](https://github.com/voidput/starorg-codex/commit/565b3cd526a431148f7f22a3891eac610ca99548))
* **enhance:** sibling descriptions for description keys stock lacks ([591e87f](https://github.com/voidput/starorg-codex/commit/591e87fea05541995d14786756f1c906df3e513a))
* **explorer:** ore quality ranges on the Sources card and where_mined ([f380be7](https://github.com/voidput/starorg-codex/commit/f380be7de0a947c0120e4d5578cf77b6b4dd6c15))
* **explorer:** Sources card with shops, loot containers and mining places ([f8b71c6](https://github.com/voidput/starorg-codex/commit/f8b71c666aeba241f3eb963eb4e0d3ee69264fc5))
* **fleet:** cargo grid SCU per hull in fleet_view ([f129d49](https://github.com/voidput/starorg-codex/commit/f129d490935d42900c53cf1d7e8e69b4710b4a8f))
* **loot:** collapsible area and item trees, readable unique area names, drops filter ([e0c08db](https://github.com/voidput/starorg-codex/commit/e0c08db10ca83abc7811dc1fc326665cfbf78a30))
* **loot:** group detail drops by where in it or place, lazy per-group rows ([98832ef](https://github.com/voidput/starorg-codex/commit/98832ef798be9355bbe73eecf7d9e0191d34c125))
* **mcp:** shop_inventory tool over a categorised UEX shop index ([c7f99e6](https://github.com/voidput/starorg-codex/commit/c7f99e627a84d701cfe97ca57ed5d951acce06a2))
* **net:** UEX market data client writing the files WHERE TO GET reads ([457f4b0](https://github.com/voidput/starorg-codex/commit/457f4b0f309880b14760e863ee5d75ddc8c7e028))
* **obtain:** best commodity sell prices in the Sources card and in game ([bea1829](https://github.com/voidput/starorg-codex/commit/bea18297e281fb1164e4166392f22ddf02e6d758))
* **patch:** curated names for added keys and a tidier readable fallback ([c1afbfa](https://github.com/voidput/starorg-codex/commit/c1afbfa2533c0d325ce06e8ca4299551b8dd4e16))
* **patches:** real name for the Reclaimer Showdown paint ([6eb5930](https://github.com/voidput/starorg-codex/commit/6eb5930526be581105b6b101b45beceb3d2db783))
* **patches:** real names for the OMNI-AFS-Sapphire flight suit colours ([0183d4c](https://github.com/voidput/starorg-codex/commit/0183d4cdeb91193242819a13f56869fb2cceceb2))
* **storage:** bound what the app keeps on disk ([df63e68](https://github.com/voidput/starorg-codex/commit/df63e68433740a5f3ec676aae00e877315690279))


### Bug Fixes

* **app:** finder waits for a running list_characters log scan ([6daecc1](https://github.com/voidput/starorg-codex/commit/6daecc1c959d7d1d278709ad53e564bd51e4f12d))
* **app:** preview says why it is empty (game data not read or cache stale) ([b0266bb](https://github.com/voidput/starorg-codex/commit/b0266bbb08d70ace79c9f89429230121381df4fc))
* **commodities:** hide the game's default density of 1, label density ([8e7abac](https://github.com/voidput/starorg-codex/commit/8e7abac3d5ed6a355103c92443bee33d6aee40fd))
* **deps:** bump url to 2.5.9 (RUSTSEC-2026-0592) ([dedaf31](https://github.com/voidput/starorg-codex/commit/dedaf31690167fefaa025ea800aaa4518c2f5b06))
* **deps:** cookie 0.18.3 (RUSTSEC-2026-0594) ([d8a3b4d](https://github.com/voidput/starorg-codex/commit/d8a3b4de2287b8b8e1fb0f1ba075b55d3bf0cf06))
* **explorer:** clear record spinner when channel switch drops in-flight load ([1c1dbdb](https://github.com/voidput/starorg-codex/commit/1c1dbdb8d85ac7dde09c2b544353d701dd7a2192))
* **loot:** a failed group load closes the group so reopening retries; blocked storage never throws ([d2baab3](https://github.com/voidput/starorg-codex/commit/d2baab3328ae92c86ef228c1e2f8079dd180b186))
* **mcp:** rebuild UEX-derived caches when the UEX files change ([c88ce40](https://github.com/voidput/starorg-codex/commit/c88ce405df391f784284f5618d72bfc0e3a46fb3))
* **memlimit:** first usage look before the worker starts ([577dd48](https://github.com/voidput/starorg-codex/commit/577dd4841c997d9cae29582d26925c557de011fb))
* **obtain:** Detailed shows what real data has ([bf1fc31](https://github.com/voidput/starorg-codex/commit/bf1fc31bea8111a22a7811badd24ce984b87270a))
* swapped Antium names, blueprint catalog uses patched names, obtain hints on disk ([ecd99c8](https://github.com/voidput/starorg-codex/commit/ecd99c886d169369dae06c8bbd201f420cfc4bee))


### Performance Improvements

* **app:** cache Strings queries, table inputs, discovery; Patches list first ([cee6bbc](https://github.com/voidput/starorg-codex/commit/cee6bbcc9a1cef1c49038ba19a0005925db6097b))
* **app:** Enhancements shows before the patch states; optimise core in debug builds ([465d677](https://github.com/voidput/starorg-codex/commit/465d6778c8dea90641ad89937df0a18a5e2a1c91))
* **app:** keep release list, storage sizes and blueprint scans warm ([8d024ba](https://github.com/voidput/starorg-codex/commit/8d024ba04b95a39055900a3da111cfac8c82e6f2))
* **app:** keep the last 3 preview tables, so toggling an option back does not rebuild ([e509917](https://github.com/voidput/starorg-codex/commit/e50991750cde2b983a6189094ab1ba89e6d8a259))
* **app:** lazy-open Explorer/Files workers, stale guards, windowed Diff/Files lists ([4e149da](https://github.com/voidput/starorg-codex/commit/4e149da6bb2575320cb77ba33fab0b4851adffd2))
* **core:** remember the global.ini hash by length and mtime in status ([a08f59d](https://github.com/voidput/starorg-codex/commit/a08f59d103ac544240dd4385fa1c8b9e2826bc06))
* optimise extraction in debug builds; key the dataset cache on extraction code, not the app version ([11b67b2](https://github.com/voidput/starorg-codex/commit/11b67b2f26b9ecadbd935de9e37a54c8fa66044e))

## [1.4.0](https://github.com/voidput/starorg-codex/compare/v1.3.0...v1.4.0) (2026-11-21)


### Features

* **explorer:** readable record view + combat card with weapon DPS ([#45](https://github.com/voidput/starorg-codex/issues/45)) ([8797161](https://github.com/voidput/starorg-codex/commit/87971613242b44a2423aa20a1cd303c4cfe57bfc))

## [1.3.0](https://github.com/voidput/starorg-codex/compare/v1.2.0...v1.3.0) (2026-11-20)


### Features

* **dev:** published LIVE builds for Diff; apply reads Data.p4k only ([#42](https://github.com/voidput/starorg-codex/issues/42)) ([f855898](https://github.com/voidput/starorg-codex/commit/f85589845c295452ef7a777f580412382a2e6f57))
* **model:** 3MF export + experimental 3D-print options ([#43](https://github.com/voidput/starorg-codex/issues/43)) ([799c383](https://github.com/voidput/starorg-codex/commit/799c38322423dac7643e013c4cbc47e88d0ba438))

## [1.2.0](https://github.com/voidput/starorg-codex/compare/v1.1.0...v1.2.0) (2026-11-17)


### Features

* **missions:** say why a [BP?] contract may not award a blueprint ([#39](https://github.com/voidput/starorg-codex/issues/39)) ([9a6dba9](https://github.com/voidput/starorg-codex/commit/9a6dba93fb1e3e0de77a89100b7f5283764f5018))

## [1.1.0](https://github.com/voidput/starorg-codex/compare/v1.0.0...v1.1.0) (2026-11-17)


### Features

* **app:** Datasets numbers with units, as the Diff shows them ([8b391d0](https://github.com/voidput/starorg-codex/commit/8b391d076f5f554e8085f55784e2e00ec2af21e4))
* **app:** Diff search across all tables and a filter on field changes ([685fd1a](https://github.com/voidput/starorg-codex/commit/685fd1aa5f8086e405ad387a8e274a3f53fa716c))
* **app:** jump between Diff and Datasets, and to the rows a record names ([6f61190](https://github.com/voidput/starorg-codex/commit/6f61190dce44112c0b426a4355f4a3ec43784c0b))
* **app:** Patreon link in the footer and About ([3d950c0](https://github.com/voidput/starorg-codex/commit/3d950c042914220277b28df097dbac51b30c0a38))
* **assets:** wear any paint and optional layer materials in a Full model export ([27ac131](https://github.com/voidput/starorg-codex/commit/27ac13125baf4484f955cd90cd5651f473cc613a))
* remove all ships first on a fleet import (prune) ([d6fe47f](https://github.com/voidput/starorg-codex/commit/d6fe47f05d1730818a623c5dcc43dc0e27d36a05))


### Bug Fixes

* **app:** link an item in every table it is in; check Diff before offering the jump ([ebe828d](https://github.com/voidput/starorg-codex/commit/ebe828d711b314ca984bbcaa59f884ae2e3133b9))
* **app:** row links on the line that names them, not as buttons on top ([41bbd2c](https://github.com/voidput/starorg-codex/commit/41bbd2c455294077c967ecd9210d8f2a1552d937))
* **app:** tell same-named diff records apart by record name; resizable record list ([2aca3f7](https://github.com/voidput/starorg-codex/commit/2aca3f705d78a9f532adbe08a4448f58aea37977))
* **blueprints:** Smart Citizen import asks for the file once ([29d3785](https://github.com/voidput/starorg-codex/commit/29d3785fed5954aff3b2f5910def038025f48194))


### Miscellaneous Chores

* release 1.1.0 ([e09e406](https://github.com/voidput/starorg-codex/commit/e09e4062dfb9dbcd62dc101f3bfa1596e8b79afe))

## [1.0.0](https://github.com/voidput/starorg-codex/compare/v0.3.0...v1.0.0) (2026-11-16)


### Features

* **app:** add a Model tab with a 3D viewer and a detail slider ([fb02da2](https://github.com/voidput/starorg-codex/commit/fb02da2a8f23906831c542fb90ba9104bebb9d19))
* **app:** add a Quest items tab to Enhancements ([9f08196](https://github.com/voidput/starorg-codex/commit/9f081967f80aeeb113fbe0c509b29ab80955edc7))
* **assets:** export the LODs a mesh has and count GLB triangles ([8b798dd](https://github.com/voidput/starorg-codex/commit/8b798ddb6fb8fc9faf65d4cc6908ec9adf5b76b9))
* **data:** extract quest items from mission spawns and hauling orders ([fc43063](https://github.com/voidput/starorg-codex/commit/fc4306303b0c7d0d5bd04f0bc3834dd35032765a))
* **enhance:** add "Used in" hints to quest items ([15266ed](https://github.com/voidput/starorg-codex/commit/15266edb46c3c7110b98676e917c72329acee160))
* **patches:** add quest-items pack for Vanduul-Tech Smugglers ([bacf14c](https://github.com/voidput/starorg-codex/commit/bacf14cc8af7edd376194180f209fac903bc36af))


### Bug Fixes

* **app:** honest detail slider, own-job Stop and a WebGL check first ([7041e45](https://github.com/voidput/starorg-codex/commit/7041e45fafd4fc7a0509d68c36f0812f4a7020b1))
* **assets:** keep a record's mesh LODs in the cache ([a77963b](https://github.com/voidput/starorg-codex/commit/a77963be82be13a7c632f774740d017b721d658b))
* **installer:** check for fusermount instead of libfuse.so.2 ([93e289a](https://github.com/voidput/starorg-codex/commit/93e289a0410d2ae1470966375bfc77fff15865c0))


### Miscellaneous Chores

* release 1.0.0 ([be540e9](https://github.com/voidput/starorg-codex/commit/be540e97bf5a898bc6e90715e4985c6b043c2127))

## [0.3.0](https://github.com/voidput/starorg-codex/compare/v0.2.0...v0.3.0) (2026-11-15)


### ⚠ BREAKING CHANGES

* the CLI is starorg-codex (starorg-strings stays an alias for 0.3.x only), the app binary is starorg-codex-app, release files are starorg-codex-v<v>-<target>, the app folders and identifier are renamed (moved automatically on first start), and hook blocks use new markers.

### Features

* **app:** blueprint finder with search, facets, sort, groups and channel colours ([ef424b4](https://github.com/voidput/starorg-codex/commit/ef424b48b571ac3eb42e5c20d8bd75bdccf0c978))
* **app:** clearer whose-blueprints hints; re-import and pull target the newest ([7e1f092](https://github.com/voidput/starorg-codex/commit/7e1f0928d4b1c58c3203c109ede44351cf9c03eb))
* **app:** collapsible Blueprints cards and finder groups ([87e74ad](https://github.com/voidput/starorg-codex/commit/87e74ad169d1001fd78aec81223e9b8ff6f9a86e))
* **app:** colour pickers, colour test and EM styles in the preview ([56094e6](https://github.com/voidput/starorg-codex/commit/56094e6d776b4bc32708743c47909d547f088730))
* **app:** dark is the default theme ([dd4c886](https://github.com/voidput/starorg-codex/commit/dd4c8863715c42c92edf8abc1cb6342eb93e52f5))
* **app:** Enhancements Blueprints tab and Markers in game card ([e2a3c08](https://github.com/voidput/starorg-codex/commit/e2a3c08cd0a16056038795e58d0d934a9d5bb8d9))
* **app:** Fix owned blueprints card with re-scan and re-import ([c1e9b3e](https://github.com/voidput/starorg-codex/commit/c1e9b3ea89d12d7eeb7cf60ff2d40d4a19315fcb))
* **app:** memory limit and dev CLI lines only with Developer mode on ([44db47a](https://github.com/voidput/starorg-codex/commit/44db47ab79ca6cff966ac669c375d43dcbe7f5fc))
* **app:** memory warning before Developer mode and MCP, limit slider ([4affae4](https://github.com/voidput/starorg-codex/commit/4affae488c0daf83faa76e5732d5b18a9e753478))
* **app:** owner fallback hint, same-name labels, picker fallback for re-import ([46c9be5](https://github.com/voidput/starorg-codex/commit/46c9be56d3326d21cc05b407f95969224fdc83a9))
* **app:** rescan_logs and Smart Citizen re-import in the contract; repair on first load ([58df117](https://github.com/voidput/starorg-codex/commit/58df117eaff495a9a930c6eb6021cac5abc1f9cf))
* **app:** Strings shows what each key is, with Show keys and shared owners ([be5e77c](https://github.com/voidput/starorg-codex/commit/be5e77c546a0309e527085af4e4ee37ab86823c1))
* **blueprints:** finder catalogue and per-channel ownership merge ([6b5b7fa](https://github.com/voidput/starorg-codex/commit/6b5b7fab89b29c382c7b583dbfe272f5c00ac04c))
* **cli:** blueprints rescan and reimport; scan reports merges and the repair ([29ce2c7](https://github.com/voidput/starorg-codex/commit/29ce2c75a0a83fc147274169aee03b574d8d2c5f))
* **contract:** blueprint_finder command with both mocks and samples ([185c0ed](https://github.com/voidput/starorg-codex/commit/185c0ed46cdef26267ccc64c625a68ccf7db1c52))
* **contract:** readable owners on Strings rows, owner search and sort ([5768746](https://github.com/voidput/starorg-codex/commit/57687466ad460ae30c632b8294bd2cc1647c6cf7))
* **core:** move the pre-0.3.0 app folders to the starorg-codex names ([56133c6](https://github.com/voidput/starorg-codex/commit/56133c62613810e35c535c286af0b19e0b46b73a))
* **dev-contract:** item finder list, sections tree, owner, compare and sound search commands ([2164ece](https://github.com/voidput/starorg-codex/commit/2164ece1a9e08c4ac296ce9e2e1529205a51441a))
* **dev-sections:** explorer sections, quick filters and media index in the dev worker ([b441e1b](https://github.com/voidput/starorg-codex/commit/b441e1be3a01759cafaa44503e2a71b6ddf73d06))
* **dev-sections:** readable sub-type labels; UNDEFINED is no sub-type facet ([90000d3](https://github.com/voidput/starorg-codex/commit/90000d3f3b2eaf477d53eaf6f9ae7e34a44338de))
* **enhance:** [Owned] is coloured by default (EM4, the game's blue) ([e702c63](https://github.com/voidput/starorg-codex/commit/e702c630c45e1a533347f4571c73568b972d233c))
* **enhance:** colour settings for mission markers and a colour test line ([996a31c](https://github.com/voidput/starorg-codex/commit/996a31cf4972d20c50b42deec9315ae318dd9fff))
* **explorer:** item finder with sections, facets, grouping, compare and sound search ([2ff0cdb](https://github.com/voidput/starorg-codex/commit/2ff0cdba0f8001a4f985caee27d3c67bcf6b9d2f))
* **memlimit:** memory limit for the Developer mode and MCP workers ([2ea65b8](https://github.com/voidput/starorg-codex/commit/2ea65b85918fcd286467c49af6fc02062da8ef69))
* rename starorg-strings to starorg-codex in binaries, packages and URLs ([3d98c30](https://github.com/voidput/starorg-codex/commit/3d98c3063def3371f875ea164d2e8ce716d7a2b2))
* **strings-owners:** key owner index from cached GameData and key shapes ([1c0cd33](https://github.com/voidput/starorg-codex/commit/1c0cd331a664d92ac494a88e7161f699d9bea17d))


### Bug Fixes

* **app:** allow blob: media in the CSP so sounds play ([700be6d](https://github.com/voidput/starorg-codex/commit/700be6d69d2d8e4ba94cad109d70e292e62af14f))
* **app:** an apply with warnings says what the warning is in Activity ([15fff45](https://github.com/voidput/starorg-codex/commit/15fff45017d9219a6cfdad6c0f4f310cb1d6bb98))
* **app:** blueprint warnings stay in view; finder collapse follow-ups ([aa12b0c](https://github.com/voidput/starorg-codex/commit/aa12b0c9b5e821dafbc815060a3fcd06b861106b))
* **app:** colour labels say the look is not verified; report via Support ([0337b32](https://github.com/voidput/starorg-codex/commit/0337b3245b25a45c6abaa95e8aeeb7fa38bf5937))
* **app:** draw display-font digits with Inter ([513670f](https://github.com/voidput/starorg-codex/commit/513670f9cd0f39ac576d441fb4ca50617f7d1122))
* **app:** finder shows ambiguous and maybe-owned rows, sits first on the screen ([a0fa4f3](https://github.com/voidput/starorg-codex/commit/a0fa4f340dbe3c92e936d1e7535398e44691dcfb))
* **app:** never point at Smart Citizen's folder; the picker opens in Downloads ([2f08c16](https://github.com/voidput/starorg-codex/commit/2f08c16df960e3b6ed335de55637709c0536fee3))
* **blueprints:** ambiguous names list candidates, unattributed set, record-name types ([c494dc4](https://github.com/voidput/starorg-codex/commit/c494dc45631c4fe32a560c7410cb29fd77eb9f46))
* **blueprints:** an unreadable set never fails auto, status or apply ([443562f](https://github.com/voidput/starorg-codex/commit/443562f158b20df755a1872b7987d49aa082367e))
* **blueprints:** auto falls back to an owner, settings follow merges, safer rescan ([3c08ff8](https://github.com/voidput/starorg-codex/commit/3c08ff83ba48aeba6dca0bb5114347f686fcd3f3))
* **blueprints:** one character per reset account, and a one-time store repair ([f66acb1](https://github.com/voidput/starorg-codex/commit/f66acb13ee5d0f50fc665f57ab0bfe99d2d7482d))
* **contract:** finder candidates, maybe owners and skipped sets; reading creates nothing ([21848ea](https://github.com/voidput/starorg-codex/commit/21848ea862fc10760a90cd2fc834c4d027289027))
* **dev-sections:** search passes, variant and section rules from review ([43c3059](https://github.com/voidput/starorg-codex/commit/43c3059377cbdf090cc222cc06cf5f56b5b8b516))
* **dev-sections:** Teach's Special ships and real display items are not variants ([eab7373](https://github.com/voidput/starorg-codex/commit/eab7373fbc7f6455d0a9b21d051a3c836bf643fe))
* **enhance:** bump OUTPUT_VERSION to 3 for per-blueprint ownership ([b0ed339](https://github.com/voidput/starorg-codex/commit/b0ed3397d1d033ae0573bbe65155ad127e217ddc))
* **enhance:** same-named blueprints never give a false [BP Owned] or marker ([f7260f0](https://github.com/voidput/starorg-codex/commit/f7260f0b9956f14062287901100d2984f588598b))
* **explorer:** matching notice, recent searches on confirm, sound cap, narrow layout ([2e1f428](https://github.com/voidput/starorg-codex/commit/2e1f42859464fc6fe957c838f7c991bad81618ab))
* **memlimit:** guard and fits use the watchdog's stop point; review fixes ([dcd3cca](https://github.com/voidput/starorg-codex/commit/dcd3cca50c3de0d8926189970be1309d541b5de8))
* **profile:** migrate old profile.toml files to only the changed settings ([5cb60df](https://github.com/voidput/starorg-codex/commit/5cb60df49494a9284339ad8aea1a67c93af38afd))
* **profile:** save a Medium tag style the preset sets otherwise ([80abb6a](https://github.com/voidput/starorg-codex/commit/80abb6a45b08529445e8c46ba003b832e4ef3613))
* **profile:** save only changed settings so new defaults reach everyone ([00fe713](https://github.com/voidput/starorg-codex/commit/00fe713c941da1ec75166ec7325848be29d90356))
* **sounds:** convert every WEM codec and say why a sound does not play ([046592d](https://github.com/voidput/starorg-codex/commit/046592d657f4f387b9837079fbebafe7f108c0c5))
* **strings-owners:** one owner per record, cleaner names, variant base names ([8377bcc](https://github.com/voidput/starorg-codex/commit/8377bcc2898dff6639da14977593e462deb458a4))
* **wizard:** explain the empty preview, show Review progress, warn before replacing another tool's file ([a900f3d](https://github.com/voidput/starorg-codex/commit/a900f3d4583d95d3073fbe81b32290ff80117abb))
* **wizard:** one game-data read at a time; Review blocks on unread data ([671ef1c](https://github.com/voidput/starorg-codex/commit/671ef1c01c3d01166aa7c986f0cc45c4eb2c6c25))


### Performance Improvements

* **dev-sections:** keep the dev worker near its old memory ([ce62317](https://github.com/voidput/starorg-codex/commit/ce623175f9af1b2e47ed5b541a7fb7c6345dbcf7))

## [0.2.0](https://github.com/voidput/starorg-strings/compare/v0.1.0...v0.2.0) (2026-11-15)


### Features

* **app:** Developer mode assets contract and backends ([eaf336a](https://github.com/voidput/starorg-strings/commit/eaf336a7bc4a8efaecdfc3411b04b01a3383a57c))
* **app:** Files browser, record sounds and model export, asset cache settings ([3f25f6b](https://github.com/voidput/starorg-strings/commit/3f25f6b9f76f2faf95c5a9ac91470e81ce381045))
* **cli:** dev extract, dev export-model, dev export-sounds ([66a8764](https://github.com/voidput/starorg-strings/commit/66a8764b1621e497dc623ade08864f24a70d5440))
* **core:** bounded, build-keyed asset cache for Developer mode ([3e4b57e](https://github.com/voidput/starorg-strings/commit/3e4b57e139037f56d9bee94f95609f33506c988e))
* **data:** asset worker for game files, sounds and models ([0c4a28b](https://github.com/voidput/starorg-strings/commit/0c4a28b991fc0ad30ef3a00576bb90bd967bcd08))


### Bug Fixes

* **core:** stop marking channels stale on app updates alone ([a8369db](https://github.com/voidput/starorg-strings/commit/a8369db59ff1d8ea6dbf557193a8aa051e89c914))
* **data:** keep a sound as WEM when ww2ogg cannot rebuild it ([ccd2c4e](https://github.com/voidput/starorg-strings/commit/ccd2c4ee2a4d6309ef62d7ef3c16492b9dce980e))
* **dev-assets:** cache never touches foreign files, checks at every use ([7cf4bb4](https://github.com/voidput/starorg-strings/commit/7cf4bb4552a3ee9d4006e10c449e2c4161cd9bdf))
* **enhance:** never write into shared LOC_* sentinel keys or placeholder text ([86592cd](https://github.com/voidput/starorg-strings/commit/86592cd610ee1fc4ad24046bf5d43eafdbcd4e11))

## [0.1.0](https://github.com/voidput/starorg-strings/compare/v0.0.2...v0.1.0) (2026-11-15)


### Features

* **app:** About dialog in the footer with license, notices and fan disclaimer ([e9df094](https://github.com/voidput/starorg-strings/commit/e9df0943dc63e8a4fffead9d60518f3776e6f478))
* **app:** group trees on the Datasets and Diff screens ([c33519a](https://github.com/voidput/starorg-strings/commit/c33519a42b45ed582a3da16d6f16c4bf1286ecc4)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Preferences step in the setup wizard ([7cfe586](https://github.com/voidput/starorg-strings/commit/7cfe586e2768732f13b3c2b00785a8d4b251ba7a))
* **app:** StarOrg Tools environment, What to sync switches and Sync now ([0d88223](https://github.com/voidput/starorg-strings/commit/0d88223850537060dda130f198ae9067659edda6)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** StarOrg Tools sign-in, pull, push and hangar in the real backend ([5b2505f](https://github.com/voidput/starorg-strings/commit/5b2505f16524cbf5d189828e9c7478358ba05ffd)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **app:** Support and Buy me a coffee links in the footer and About ([254a06a](https://github.com/voidput/starorg-strings/commit/254a06ade4b2ab5a0bd3857419f040d570fa6e77))
* **contract:** dev_groups and dev_diff_groups for the group trees ([140f5af](https://github.com/voidput/starorg-strings/commit/140f5af132d7f324a73d74b1a15cc0e9651dd80d)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **scripts:** add a user-local Linux installer for the CLI and AppImage ([46ec8ea](https://github.com/voidput/starorg-strings/commit/46ec8ea5cd80029fa2765755ea868cb03233ef6d))


### Bug Fixes

* **dev:** name dataset values by record name; tell shared titles apart; name list positions ([ac55e0f](https://github.com/voidput/starorg-strings/commit/ac55e0f37ffe154107e608dd70dacb5673b2fdae)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **dev:** name records from their own name fields only; Show IDs hides inline stock text ([8523ece](https://github.com/voidput/starorg-strings/commit/8523ece66107609faab92adc10d4f0bc6b6c323a)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **dev:** total order for group-tree siblings; Diff tree follows Match by; fit 1024 ([b51dd16](https://github.com/voidput/starorg-strings/commit/b51dd16fbef06f7e6664875fdea804301bd4fa4c)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)
* **starorg:** honest sign-out, STARORG_ENV only on a bare sync set, safer push times ([06057ce](https://github.com/voidput/starorg-strings/commit/06057ce4dbce254c0a4a40eb0b08f49a24129f95)), closes [#15](https://github.com/voidput/starorg-strings/issues/15)

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
