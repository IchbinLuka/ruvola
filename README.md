<p align="center">
<img src="assets/logo.png" alt="RuVoLA Logo" width="150"/>
<h1 align="center">RuVoLA</h1>
</p>


RuVoLA (**Ru**sty **Vo**cabulary **L**earning **A**pplication) is a TUI based application for learning vocabulary written in Rust. As opposed to flashcard programs like [vocage](https://github.com/proycon/vocage) or [anki](https://apps.ankiweb.net/), the user is here required to type the vocabularies similar to the [phase6](https://www.phase-6.de/) platform. 

To ensure that words with a higher error rate are repeated more often, RuVoLA employs a system similar to vocage where the words are moved between decks with each deck having a different presentation interval. 

![Example usage of RuVoLA](assets/showcase.gif)

## Features
- Similar to vocage, the data is stored in a simple plain-text tab-separated values format (TSV)
    - Here, however, the TSV files can only contain two columns
    - The learning progress is also directly stored in the TSV file, allowing you to store the vocabularies and learning progress in a version control system like git
- Multiple vocabulary files can be loaded at once. This allows for grouping of vocabularies into different levels/domains/etc.
- RuVoLA forces the user to type out the vocabulary to better memorize them.
- RuVoLA can be configured using a simple configuration file (see below). 
- Special character support: Some languages have special characters that are not supported by all keyboard layouts. RuVoLA allows you to define sets of special characters for each language.
- Written in Rust

## Installation
1. Make sure you have the Rust toolchain installed. You can install it using [rustup](https://rustup.rs/).
2. Clone the repository:
```bash
git clone https://github.com/IchbinLuka/ruvola.git
```
3. In the cloned directory, run:
```bash
cargo install --path .
```

## Usage
For a full list of parameters, run `ruvola -h`.
```bash
> ruvola vocabs.tsv
```

## Keybindings
| Key | Action |
|------------|--------|
| `e`        | Enter Edit Mode | 
| `Enter`    | Submit the current buffer |
| `w`        | Save and quit |
| `Q`        | Quit without saving |
| `a`        | Accept anyway (if answer was marked as wrong) |
| `s`        | Skip the current card |
| `Esc`      | Stop editing |
| `Ctrl + Space` | Show all special characters (in edit mode) |
| `Ctrl + <Key>` | Show special characters for the given key (in edit mode) | 

## Configuration file
RuVoLA allows customization using a configuration file. This file needs to be created manually. If the file does not exist, RuVoLA will use the default config. This configuration file should be placed in:
- `$XDG_CONFIG_HOME/ruvola/config.toml` on Linux
- `$HOME/Library/Application Support/ruvola/config.toml` on MacOS
- `%APPDATA%/ruvola/config.toml` on Windows


Below is an example configuration file, additionally providing special characters for german, italian and french. 

```toml
[memorization]
# For vocabularies that have not been presented to the user yet, you
# can enable a memorization round where both the query and answer are
# shown.
do_memorization_round = true
memorization_reversed = false

[validation]
# The maximum edit distance for a word to be considered correct
error_tolerance = 2
# For words with a length of less than this, the error tolerance is set to 0
tolerance_min_length = 5

[deck_config]
# The interval of each deck in days
deck_durations = [0, 1, 7, 14, 30, 60, 90]

[special_letters]
de = [
    { base = "a", special = ["Ä", "ä"] }, 
    { base = "o", special = ["Ö", "ö"] },
    { base = "u", special = ["Ü", "ü"] },
    { base = "s", special = ["ẞ", "ß"] },
]
it = [
    { base = "a", special = ["À", "à"] },
    { base = "e", special = ["É", "é", "È", "è"] },
    { base = "i", special = ["Ì", "ì"] },
    { base = "o", special = ["Ò", "ò"] },
    { base = "u", special = ["Ù", "ù"] },
]
fr = [
    { base = "a", special = ["À", "à", "Â", "â", "Æ", "æ"] },
    { base = "c", special = ["Ç", "ç"] },
    { base = "e", special = ["É", "é", "È", "è", "Ê", "ê"] },
    { base = "i", special = ["Î", "î", "Ï", "ï"] },
    { base = "o", special = ["Ô", "ô", "Œ", "œ"] },
    { base = "u", special = ["Ù", "ù", "Û", "û"] },
]
```

## Vocabulary file format
The vocabulary file is a simple tab-separated values (TSV) file with two columns where each column corresponds to one language. The first line of the file need to be a header that specifies which column is which language. Below is a minimal example of a vocabulary file. 
```tsv
de	en
Hallo	Hello
Tschüss	Bye
Bier	Beer
```
