# Changelog

## [2.0.0](https://github.com/BlackDark/fork-clevercoffee/compare/v2.0.0-alpha.2...v2.0.0) (2026-10-31)


### ⚠ BREAKING CHANGES

* remove the C++ firmware, PlatformIO, Wokwi and the old docs

### Features

* **api:** JSON bodies, nested config files, forms and multipart uploads ([8b9738f](https://github.com/BlackDark/fork-clevercoffee/commit/8b9738ff976a57bceeccebee6ca595556001faa2))
* **display:** all OLED screens on embedded-graphics, plus a screen generator ([7a6b9b6](https://github.com/BlackDark/fork-clevercoffee/commit/7a6b9b6c4b56fbf367d4ca35c3fe9ad9fc1f82a0))
* **drivers:** TSIC, DS18B20, HX711, ABP2, OLED and BLE scale protocols ([1c8e07e](https://github.com/BlackDark/fork-clevercoffee/commit/1c8e07e04b5e28d45f186d2a9910b515703d9615))
* **firmware:** ESP32 firmware on Embassy wiring the host-tested crates ([5c1e028](https://github.com/BlackDark/fork-clevercoffee/commit/5c1e028f90379e583da2395fefa1bffd97f84528))
* **firmware:** one HTTP worker pool, restart after the WiFi fallback, stack monitor ([3080326](https://github.com/BlackDark/fork-clevercoffee/commit/3080326ed2e687e9930e60b6173673743dc46d3d))
* **firmware:** timing debug logs the longest gap between control ticks ([6b9148e](https://github.com/BlackDark/fork-clevercoffee/commit/6b9148eb87710f9057caa6da067b8dd956734d0e))
* justfile, provisioning tool, firmware-only web updates in the UI ([f9f1056](https://github.com/BlackDark/fork-clevercoffee/commit/f9f10563c8d6b7f8f2c67c68f783a05c748713ea))
* **mqtt:** one entity table for topics, commands and HA discovery ([63a0727](https://github.com/BlackDark/fork-clevercoffee/commit/63a07270775f3469468b9ee98ffede478c638da9))
* **net:** listen backlog with a smoltcp listener instead of dropped SYNs ([ba840e0](https://github.com/BlackDark/fork-clevercoffee/commit/ba840e081687f41970d856cfcad70cf865f7d821))
* **net:** listen backlog, more HTTP workers, capped event streams ([c71bc17](https://github.com/BlackDark/fork-clevercoffee/commit/c71bc17d719885bd086d0b6334f44b9e06d12e7b))
* scaffold Rust workspace with PID and core machine logic ([1ba1266](https://github.com/BlackDark/fork-clevercoffee/commit/1ba1266e0854f8cb5c4637e19b29b6dcbf488717))
* **tools:** bench smoke tests and bench provisioning profile ([c9ef7fc](https://github.com/BlackDark/fork-clevercoffee/commit/c9ef7fcc25d718c1cbb7b2ad30ffe4d2ba483427))
* **ui:** persistent standby toast, alert warning variant and action slot ([7d06534](https://github.com/BlackDark/fork-clevercoffee/commit/7d06534055dd3be495375c7c56d953b0c1a895d8))
* **ui:** replace React UI with the Svelte port from main ([50bf3c6](https://github.com/BlackDark/fork-clevercoffee/commit/50bf3c68b58c8cb399bf876c5c4da5d6440bc881))
* **web:** admit a request only with 8 KB of heap free ([668defd](https://github.com/BlackDark/fork-clevercoffee/commit/668defddbcd8acce9903896d58a65b506f4701a9))
* **web:** HTTP routes over a hardware-free backend, tested end to end ([61ff129](https://github.com/BlackDark/fork-clevercoffee/commit/61ff129aca4d90176fda685dc43cc2b2200567ad))


### Bug Fixes

* **core:** a sensor error that began in standby recovers into standby ([58e7f34](https://github.com/BlackDark/fork-clevercoffee/commit/58e7f34d27ef5b9531bde59b41787285131f2747))
* **core:** no WATER_TANK_EMPTY at boot with a full tank ([f85d9b1](https://github.com/BlackDark/fork-clevercoffee/commit/f85d9b1c820401d8b7aa787a98945ef2b857d59b))
* **core:** report 0 % heater power while an interlock holds the heater off ([9a805f4](https://github.com/BlackDark/fork-clevercoffee/commit/9a805f43d1dbc934ca325009107b58020b9a7bf1))
* create target/ before saving the app image ([f0f39a0](https://github.com/BlackDark/fork-clevercoffee/commit/f0f39a0aa59b4f56ba3bbb4b3b89950a03887fc3))
* **firmware:** fit request handling into the ESP32's stack and heap ([d357282](https://github.com/BlackDark/fork-clevercoffee/commit/d357282ac5f5005ca31b3eb9c0f100cc292c4cde))
* **firmware:** minFreeHeap is the allocator's exact high-water mark ([ec96ea2](https://github.com/BlackDark/fork-clevercoffee/commit/ec96ea2d2bad73d3d3e3786b04514a99544d0de4))
* **firmware:** reach main and start WiFi on the ESP32 ([b42db20](https://github.com/BlackDark/fork-clevercoffee/commit/b42db2022d209461451af84f46c00993cba5dab6))
* **firmware:** stack monitor painted over main's own variables ([e355f3b](https://github.com/BlackDark/fork-clevercoffee/commit/e355f3b0796c2b89d433bcb44f7833c153145c6c))
* **just:** flash resets otadata, size on macOS, bench switches enabled ([7adcf80](https://github.com/BlackDark/fork-clevercoffee/commit/7adcf80ded79e2144fb3c08a84ee5808a8add8e1))
* **just:** provision keeps an SSID with spaces as one argument ([2cf1ac6](https://github.com/BlackDark/fork-clevercoffee/commit/2cf1ac677b4de387c6000998511a2b119310757d))
* **mdns:** answer with an NSEC record that the host has no IPv6 address ([f7fd0ae](https://github.com/BlackDark/fork-clevercoffee/commit/f7fd0ae2ddec7b1a7eee3b6c3f4553f8d6b5022f))
* **mqtt:** ping when a publish round sent nothing ([1a12998](https://github.com/BlackDark/fork-clevercoffee/commit/1a12998ac635db6ee928ca30f0ec190790a32102))
* **web:** health length, long firmware uploads, write timeout, hang reply ([4aa9327](https://github.com/BlackDark/fork-clevercoffee/commit/4aa9327719a84f8472039b6890de49f9a6f63b4f))


### Performance

* **core:** history points in hundredths, 3.6 KB instead of 7.2 KB ([56560fd](https://github.com/BlackDark/fork-clevercoffee/commit/56560fdb7f20ab16404580030f3c274defd5b549))
* **firmware:** 80 KB main-RAM heap without Bluetooth ([87672fb](https://github.com/BlackDark/fork-clevercoffee/commit/87672fbaeee15a555e96787a1be7156bc8ca337d))
* **web:** stream JSON in windows that stop early, 4 KB send buffer, no keep-alive ([4d1ebae](https://github.com/BlackDark/fork-clevercoffee/commit/4d1ebaefc4402a678cf9213eed9c731d10f000c1))


### Refactoring

* **ui:** button variants, shared error/command helpers, split system actions ([b98592a](https://github.com/BlackDark/fork-clevercoffee/commit/b98592aea8a6ceba6cfb9cc9915e5c170f1086ac))
* **ui:** use shadcn Card header, Sheet menu and Skeleton ([1f085ac](https://github.com/BlackDark/fork-clevercoffee/commit/1f085ac0a021a5c76cf5150947b9fc353f53124f))


### Documentation

* 4 h soak result, DS18B20 read error rate ([74a2e14](https://github.com/BlackDark/fork-clevercoffee/commit/74a2e149955c5a516b843f973140b10ffc9186d8))
* bench bring-up results, RAM and timing measurements ([ac5cc7c](https://github.com/BlackDark/fork-clevercoffee/commit/ac5cc7c22f97820aeeec4991c0633add4cd2ea95))
* fresh README, AGENTS.md, glossary, firmware/bench/API guides and ADRs ([795273d](https://github.com/BlackDark/fork-clevercoffee/commit/795273da4dc089f13470ec15ec2e6e6425bdd320))
* listen backlog, HTTP timings and exact heap marks (ADR 0008) ([a630c81](https://github.com/BlackDark/fork-clevercoffee/commit/a630c81df2694d7e136dc82e0aef0ee11c48cdd0))
* release process and toolchain version in AGENTS.md ([f3fc4c3](https://github.com/BlackDark/fork-clevercoffee/commit/f3fc4c36bfe1ab718e476e0c4588cb657d33311e))
* remeasured heap, stack and upload speed; fallback confirmed ([f4a7f97](https://github.com/BlackDark/fork-clevercoffee/commit/f4a7f973a398d37ac1283288db925c19491148f2))
* upstream status of the vendored patches ([7d3b216](https://github.com/BlackDark/fork-clevercoffee/commit/7d3b216f201121042802f50a31f79a7c84393985))


### Chores

* remove the C++ firmware, PlatformIO, Wokwi and the old docs ([0975ce6](https://github.com/BlackDark/fork-clevercoffee/commit/0975ce6a7b6333fc76ae3528129416324586a478))
