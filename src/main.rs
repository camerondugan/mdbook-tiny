extern crate mdbook;
extern crate pulldown_cmark;

use copy_dir::copy_dir;
use mdbook::renderer::RenderContext;
use mdbook::{BookItem, book::Chapter};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};
use regex::Regex;
use std::fmt::Write;
use std::io::Write as ioWrite;
use std::path::Path;
use std::sync::LazyLock;
use std::{
    fs::{self, File},
    io::{self, BufWriter},
    path::PathBuf,
};

fn main() {
    let mut stdin = io::stdin();
    let ctx = RenderContext::from_json(&mut stdin).unwrap();

    let _ = fs::create_dir_all(&ctx.destination);

    // Support rss extension
    let _ = fs::copy(
        ctx.source_dir().join("rss.xml"),
        ctx.destination.join("rss.xml"),
    );
    // Support my old feed location
    let _ = fs::copy(
        ctx.source_dir().join("rss.xml"),
        ctx.destination.join("feed.xml"),
    );
    // Support my game: Arbidor
    let _ = copy_dir(
        ctx.source_dir().join("arbidor"),
        &ctx.destination.join("arbidor"),
    );
    // Load my css
    let _ = copy_dir(
        ctx.source_dir().parent().unwrap().join("css").join(""),
        ctx.destination.join("css"),
    );

    for item in ctx.book.iter() {
        if let BookItem::Chapter(ref ch) = *item {
            if let Some(path) = &ch.path {
                // Write to a file
                let mut depth = 0;
                let mut tmp = Some(path.as_path()); // relative path from summary.md
                while tmp.is_some() {
                    depth += 1; // how far down is our file?
                    tmp = tmp.unwrap().parent();
                }
                depth -= 2; // unsure why this is the case

                parse(
                    &ch,
                    &ctx.destination.join(&path.with_extension("html")),
                    depth,
                );
            }
        }
    }
    // // Set my index.html
    // let _ = fs::copy(
    //     ctx.destination.join("getting-started.html"),
    //     ctx.destination.join("index.html"),
    // );
}

fn parse(ch: &Chapter, out_path: &PathBuf, depth: u8) {
    // Create parser with example Markdown text.
    if ch.is_draft_chapter() {
        let parser = custom_parser(&"# Draft Chapter\nNot released yet...\nShhhhh...");
        write_html(parser, out_path, depth);
    } else {
        let parser = custom_parser(&ch.content);
        write_html(parser, out_path, depth);
    }
}

fn write_html(parser: Parser, out_path: &PathBuf, depth: u8) {
    let _ = fs::create_dir_all(&out_path.parent().unwrap());
    let f = File::create(out_path).unwrap();
    let mut writer = BufWriter::new(f);
    let mut css_path = "css/pico.classless.jade.min.css".to_owned();

    for _ in 0..depth {
        css_path = format!("../{}", css_path)
    }

    let _ = writer.write(
        format!("<!doctype html>\n<head><link rel=\"stylesheet\" href=\"{css_path}\"></head>")
            .as_bytes(),
    );

    let mutated = parser.map(|event| adjust_links(event, None));
    // .map(|event| match event {
    //     _ => event,
    // });
    let _ = pulldown_cmark::html::write_html_io(&mut writer, mutated);
}

// my personal preferences of options (smart punctuation breaks my book)
fn custom_parser(input: &str) -> Parser {
    let options = Options::all().difference(Options::ENABLE_SMART_PUNCTUATION);
    return Parser::new_ext(input, options);
}

// Stolen from mdbook's non-public fn
fn adjust_links<'a>(event: Event<'a>, path: Option<&Path>) -> Event<'a> {
    static SCHEME_LINK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9+.-]*:").unwrap());
    static MD_LINK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?P<link>.*)\.md(?P<anchor>#.*)?").unwrap());

    fn fix<'a>(dest: CowStr<'a>, path: Option<&Path>) -> CowStr<'a> {
        if dest.starts_with('#') {
            // Fragment-only link.
            if let Some(path) = path {
                let mut base = path.display().to_string();
                if base.ends_with(".md") {
                    base.replace_range(base.len() - 3.., ".html");
                }
                return format!("{base}{dest}").into();
            } else {
                return dest;
            }
        }
        // Don't modify links with schemes like `https`.
        if !SCHEME_LINK.is_match(&dest) {
            // This is a relative link, adjust it as necessary.
            let mut fixed_link = String::new();
            if let Some(path) = path {
                let base = path
                    .parent()
                    .expect("path can't be empty")
                    .to_str()
                    .expect("utf-8 paths only");
                if !base.is_empty() {
                    write!(fixed_link, "{base}/").unwrap();
                }
            }

            if let Some(caps) = MD_LINK.captures(&dest) {
                fixed_link.push_str(&caps["link"]);
                fixed_link.push_str(".html");
                if let Some(anchor) = caps.name("anchor") {
                    fixed_link.push_str(anchor.as_str());
                }
            } else {
                fixed_link.push_str(&dest);
            };
            return CowStr::from(fixed_link);
        }
        dest
    }

    fn fix_html<'a>(html: CowStr<'a>, path: Option<&Path>) -> CowStr<'a> {
        // This is a terrible hack, but should be reasonably reliable. Nobody
        // should ever parse a tag with a regex. However, there isn't anything
        // in Rust that I know of that is suitable for handling partial html
        // fragments like those generated by pulldown_cmark.
        //
        // There are dozens of HTML tags/attributes that contain paths, so
        // feel free to add more tags if desired; these are the only ones I
        // care about right now.
        static HTML_LINK: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"(<(?:a|img) [^>]*?(?:src|href)=")([^"]+?)""#).unwrap());

        HTML_LINK
            .replace_all(&html, |caps: &regex::Captures<'_>| {
                let fixed = fix(caps[2].into(), path);
                format!("{}{}\"", &caps[1], fixed)
            })
            .into_owned()
            .into()
    }

    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: fix(dest_url, path),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: fix(dest_url, path),
            title,
            id,
        }),
        Event::Html(html) => Event::Html(fix_html(html, path)),
        Event::InlineHtml(html) => Event::InlineHtml(fix_html(html, path)),
        _ => event,
    }
}
