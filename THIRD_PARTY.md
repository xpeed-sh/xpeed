# Third-party software

Xpeed uses open-source Rust libraries. The portable Windows package includes the dependency license declarations in `DEPENDENCIES.txt` and copies of license/notice files from their source packages in `licenses/`.

The main components are:

- egui / eframe / epaint: MIT OR Apache-2.0 — https://github.com/emilk/egui
- egui_tiles: MIT OR Apache-2.0 — https://github.com/rerun-io/egui_tiles
- alacritty_terminal: Apache-2.0 — https://github.com/alacritty/alacritty
- portable-pty: MIT — https://github.com/wezterm/wezterm
- serde / serde_json: MIT OR Apache-2.0 — https://github.com/serde-rs/serde
- png: MIT OR Apache-2.0 — https://github.com/image-rs/image-png
- dirs: MIT OR Apache-2.0 — https://github.com/dirs-dev/dirs-rs
- reqwest: MIT OR Apache-2.0 — https://github.com/seanmonstar/reqwest
- keyring: MIT OR Apache-2.0 — https://github.com/open-source-cooperative/keyring-rs
- zeroize: MIT OR Apache-2.0 — https://github.com/RustCrypto/utils
- rand: MIT OR Apache-2.0 — https://github.com/rust-random/rand
- rustls: Apache-2.0 OR ISC OR MIT — https://github.com/rustls/rustls

Built-in fonts are provided by epaint_default_fonts, including Hack, Ubuntu, Noto Emoji and emoji-icon-font. Their notices and license texts are included from the font package in the distribution's `licenses/epaint_default_fonts-*` directory.

These third-party licenses apply to their respective components. Xpeed's own source is licensed under the MIT license in `LICENSE`.

- egui_commonmark / pulldown-cmark: MIT OR Apache-2.0 - CommonMark response rendering.
- chrono: MIT OR Apache-2.0 - local message timestamps.
