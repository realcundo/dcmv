# dcmv

`dcmv` is a cross-platform terminal-based DICOM image viewer. It displays DICOM images and their metadata directly in the terminal.

This is an initial non-interactive implementation. The purpose is a quick preview of DICOM files without leaving the terminal.

In modern terminals that support image protocols (e.g., Kitty, Ghostty, iTerm2, WezTerm), images are displayed in higher resolution.

## Installation

Use [Cargo](https://rustup.rs) to install `dcmv` from this git repository:

```bash
cargo install dcmv --git https://github.com/realcundo/dcmv
```

## Usage

```bash
dcmv <FILE> [<FILE2> ..]
```

### Options

- `<FILE>`: One or more DICOM file paths.
- `-W`, `--width <WIDTH>` (optional): Set the output width in terminal columns.
- `-H`, `--height <HEIGHT>` (optional): Set the output height in terminal rows.
- `-v`, `--verbose` (optional): Show DICOM metadata.

## License

`dcmv` is dual-licensed under both the MIT and Apache 2.0 licenses. See `LICENSE-MIT` and `LICENSE-APACHE` for full details.
