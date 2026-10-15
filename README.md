# Docker Resource TUI

[![Language](https://img.shields.io/github/languages/top/ulrichheringer/docker-resource-tui?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/github/license/ulrichheringer/docker-resource-tui)](LICENSE)

View Docker containers and their CPU, memory, and network usage in a terminal interface. Metrics refresh every two seconds, and you can select containers and sort the list.

## Install

Requires Rust 1.88 or newer.

```sh
cargo install --path .
```

## Run

Make sure the Docker daemon is running, then start the interface:

```sh
docker-resource-tui
```

Use ↑/↓ or `j`/`k` to select a container, `s` to change the sort field, `o` to reverse sort order, and `q` to quit.
