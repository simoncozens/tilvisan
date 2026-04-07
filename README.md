# tilvisan

`tilvisan` is a tool for autohinting TrueType fonts. It began life as a port of `ttfautohint` to Rust, but now uses the autohint implementation in `skrifa` instead. It is designed to be used both as a library and as a command-line tool.

It supports the `ttfautohint` command-line interface and control file mechanism. The control file mechanism allows users to specify custom hinting instructions for specific glyphs, which can be useful for fine-tuning the hinting of a font.

## Name

Following the [fontations](https://github.com/googlefonts/fontations) Old Norse naming scheme, [*til-vísan*](https://cleasby-vigfusson-dictionary.vercel.app/word/til-visan) is the Old Norse word for "guidance", "direction" or "instruction".

## License

`tilvisan` is licensed under the same terms as Rust (MIT + Apache-2.0). See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.