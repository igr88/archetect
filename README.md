# archetect

![Rust](https://github.com/archetect/archetect/workflows/Rust/badge.svg)

A powerful code-centric content generator

Modules:
* [archetect_cli](archetect-cli/README.md)
* [archetect_core](archetect-core/README.md)

## Quick Start

For more in-depth instruction for getting started with Archetect, see the Documentation section below.

### Installation

Archetect is a CLI application, and can either be installed by downloading a pre-built binary from Archetect's 
[Releases Page](https://github.com/archetect/archetect/releases/latest), or by installing with 
[Rust Lang's](https://rustup.rs/) build too, cargo:

```shell
cargo install architect --force
```

Once you have Archetect successfully installed and added to your shell's path, you can test that everything is working while
also initilizing some default settings by generating those setting with Archetect itself:

```shell
archetect render https://github.com/archetect/archetect-initializer.git ~/.archetect/
```

This will prompt you for your name and email address, and write this into files within the `~/.archetect`, which you can
inspect.

From this point, browse the archetypes and catalogs within the [Archetect Github organization](https://github.com/archetect) 
for some pre-made archetypes you can use immediately or for inspiration in making your own.  The README.md files commonly
have the archetect command line example that can be copy/pasted to your shell to render a new project.

Example:

```shell
# To generate a Rust microservice using Actix and Diesel
archetect render https://github.com/archetect/archetype-rust-service-actix-diesel-workspace.git

# To select from a catalog of archetypes using a command line menu system
archetect catalog --source https://github.com/archetect/catalog-rust.git
```

## Documentation 
[Archetect Documentation](https://archetect.github.io/archetect.html)

## Binary Releases
[Releases for OSX, Windows, and Linux](https://github.com/archetect/archetect/releases)

## Installation
[Installation Guide](https://archetect.github.io/getting_started/installation.html)

## *Usage*
```
archetect 0.3.1
Jimmie Fulton <jimmie.fulton@gmail.com>


USAGE:
    archetect [FLAGS] [OPTIONS] <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -o, --offline    Only use directories and already-cached remote git URLs
    -V, --version    Prints version information
    -v, --verbose    Increases the level of verbosity

OPTIONS:
    -a, --answer <key=value>...    Supply a key=value pair as an answer to a variable question.
    -A, --answer-file <path>...    Supply an answers file as answers to variable questions.
    -s, --switch <switches>...     Enable switches that may trigger functionality within Archetypes

SUBCOMMANDS:
    cache          Manage/Select from Archetypes cached from Git Repositories
    catalog        Select From a Catalog
    completions    Generate shell completions
    help           Prints this message or the help of the given subcommand(s)
    render         Creates content from an Archetype
    system         archetect system configuration
```
