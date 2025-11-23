# mdbook-tiny

An alternative backend to mdbook that generates minimal HTML from your MarkDown source.

Pages typically generated are under 14 kilobytes, much smaller than the pages you get from mdbook directly.

## What you gain
Pages without extra assets load on first response from server and get a near perfect lighthouse speed score in most cases.

You can keep generating your content both as a mdbook and in this tiny HTML format.

## What you lose

To get this small size, you lose search, sidebar, some code highlighting languages, code block clipboard buttons, rust playground, click to PDF.

## Setup

To use, you need to have the `mdbook-tiny` binary available in your terminal's PATH.

### Install from GitHub:

```bash
git clone https://github.com/camerondugan/mdbook-tiny.git
cargo install --path ./mdbook-tiny
```

### or Install from cargo:
```bash
cargo install mdbook-tiny
```

## Usage
Add it as a backend in book.toml
```toml
[output.tiny]
nav-separator = " - "
# relative to your src folder
css-paths = ["css/pico.classless.min.css"]
index = "getting-started.md"
extra-nav.Blog = "blog.md"
extra-nav.Projects = "projects.md"
```
