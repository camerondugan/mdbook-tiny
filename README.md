# mdbook-tiny

An alternative backend to mdbook that generates minimal html from your md source.

Pages typically generated are under 14kb, much smaller than the pages you get from mdbook directly.

## Trade-offs

Pages without extra assets load on first response from server and get a near perfect lighthouse speed score.

You can keep generating your content both as an mdbook and in this tiny html format.

To get this small size, you lose search, sidebar, code highlighting, code clipboard buttons, rust playground, click to pdf.

## Setup

Install:
```bash
cargo install mdbook-tiny
```

Add it as a backend in book.toml:
```toml
[output.tiny]
nav-separator = " - "
# relative to your src folder
css-path = "css/pico.classless.min.css"
index = "getting-started.md"
extra-nav.Blog = "blog.md"
extra-nav.Projects = "projects.md"
```
