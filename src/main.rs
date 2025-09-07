extern crate mdbook;
extern crate pulldown_cmark;
extern crate serde;
extern crate serde_derive;

use mdbook::renderer::RenderContext;
use mdbook::utils::fs::copy_files_except_ext;
use mdbook::{BookItem, book::Chapter};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::io::Write as ioWrite;
use std::path::Path;
use std::sync::LazyLock;
use std::{
    fs::{self, File},
    io::{self, BufWriter},
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TinyConfig {
    pub css_path: String,
    pub nav_separator: String,
    pub index: Option<String>,
    pub nav_bottom_empty: bool,
}

impl Default for TinyConfig {
    fn default() -> Self {
        Self {
            css_path: Default::default(),
            nav_separator: " - ".to_string(),
            index: None,
            nav_bottom_empty: true,
        }
    }
}

fn main() {
    let mut stdin = io::stdin();
    let ctx = RenderContext::from_json(&mut stdin).unwrap();
    let cfg: TinyConfig = ctx
        .config
        .get_deserialized_opt("output.tiny")
        .unwrap_or_default()
        .unwrap_or_default();

    let _ = fs::create_dir_all(&ctx.destination);

    // Copy over other files
    let _ = copy_files_except_ext(&ctx.source_dir(), &ctx.destination, true, None, &["md"]);

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
                    &ctx,
                    &cfg,
                    &ch,
                    &ctx.destination.join(&path.with_extension("html")),
                    depth,
                );
            }
        }
    }
    // TODO: Get first item from summary.md and make it the index
    // Optional: configurable

    // Set my index.html
    if let Some(index) = cfg.index {
        let html_index = PathBuf::from(index).with_extension("html");
        let _ = fs::copy(
            ctx.destination.join(html_index),
            ctx.destination.join("index.html"),
        );
    };
}

fn parse(ctx: &RenderContext, cfg: &TinyConfig, ch: &Chapter, out_path: &PathBuf, depth: u8) {
    // Create parser with example Markdown text.
    let parser = match ch.is_draft_chapter() {
        true => custom_parser(&"# Draft Chapter\nNot released yet...\nShhhhh..."),
        false => custom_parser(&ch.content),
    };
    write_html(&ctx, &cfg, &ch, parser, out_path, depth);
}

fn write_html(
    ctx: &RenderContext,
    cfg: &TinyConfig,
    ch: &Chapter,
    parser: Parser,
    out_path: &PathBuf,
    depth: u8,
) {
    let _ = fs::create_dir_all(&out_path.parent().unwrap());
    let f = File::create(out_path).unwrap();
    let mut writer = BufWriter::new(f);

    let css_content = match &cfg.css_path {
        v if v.len() == 0 => v.to_string(), // if empty leave empty
        val => format!(
            "<style>{}</style>",
            fs::read_to_string(ctx.source_dir().join(val)).unwrap()
        ),
    };

    let title_head = match &ctx.config.book.title {
        Some(name) => format!(
            "<title>{} - {name}</title><meta name=\"title\" content=\"{name}\">",
            ch.name
        ),
        None => "".to_string(),
    };
    let description_head = match &ctx.config.book.description {
        Some(desc) => format!("<meta name=\"description\" content=\"{desc}\">"),
        None => "".to_string(),
    };
    let nav = parent_links(ctx, cfg, ch, depth).join(&cfg.nav_separator);
    let title = match &ctx.config.book.title {
        Some(name) => format!("<h1>{name}</h1>"),
        None => "".to_string(),
    };

    let _ = writer.write(
        format!("<!doctype html>\n<head>{title_head}{description_head}<meta charset=\"utf8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">{css_content}</head><body><header>{nav}</header><main>{title}")
            .as_bytes(),
    );

    let mutated = parser.map(|event| adjust_links(event, None));
    // customize your md to html conversion
    // .map(|event| match event {
    //     _ => event,
    // });
    let _ = pulldown_cmark::html::write_html_io(&mut writer, mutated);

    let bottom_nav = child_links(ctx, cfg, ch, depth).join(&cfg.nav_separator);
    let _ = writer.write(format!("</main><footer>{bottom_nav}</footer></body></html>").as_bytes());
}

fn parent_links(ctx: &RenderContext, _cfg: &TinyConfig, ch: &Chapter, depth: u8) -> Vec<String> {
    let mut links: Vec<String> = vec![format!(
        "<a href=\"{}\">Home</a>",
        apply_depth("index.html".to_string(), depth)
    )];
    let parents = &ch.parent_names;
    if parents.len() == 0 {
        return links;
    }
    ctx.book.sections.iter().for_each(|sec| match sec {
        BookItem::Chapter(ich) => {
            if parents.iter().any(|p| p.eq(&ich.name)) {
                if let Some(path) = ich.path.clone() {
                    links.push(format!(
                        "<a href=\"{}\">{}</a>",
                        apply_depth(
                            path.with_extension("html")
                                .to_str()
                                .unwrap_or("")
                                .to_string(),
                            depth
                        ),
                        ich.name
                    ))
                }
            }
        }
        _ => {}
    });
    return links;
}

fn child_links(ctx: &RenderContext, cfg: &TinyConfig, ch: &Chapter, depth: u8) -> Vec<String> {
    let mut links: Vec<String> = vec![];
    let parents = &ch.sub_items;
    if parents.len() == 0 {
        return match cfg.nav_bottom_empty {
            true => links,
            false => parent_links(ctx, cfg, ch, depth),
        };
    }
    ctx.book
        .iter()
        .filter(|item| parents.contains(item))
        .for_each(|item| match item {
            BookItem::Chapter(ich) => {
                if let Some(path) = ich.path.clone() {
                    links.push(format!(
                        "<a href=\"{}\">{}</a>",
                        apply_depth(
                            path.with_extension("html")
                                .to_str()
                                .unwrap_or("")
                                .to_string(),
                            depth
                        ),
                        ich.name,
                    ));
                }
            }
            _ => {}
        });
    return links;
}

fn apply_depth(path: String, depth: u8) -> String {
    let mut ans = path;
    for _ in 0..depth {
        ans = format!("../{ans}");
    }
    return ans;
}

// my personal preferences of options (smart punctuation breaks my book)
fn custom_parser(input: &str) -> Parser {
    let options = Options::all().difference(Options::ENABLE_SMART_PUNCTUATION);
    return Parser::new_ext(input, options);
}

// this is here only because mdbook didn't mark it as pub
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
