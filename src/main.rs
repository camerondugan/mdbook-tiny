extern crate mdbook;
extern crate pulldown_cmark;
extern crate serde;
extern crate serde_derive;

use highlight_pulldown::highlight;
use mdbook::renderer::RenderContext;
use mdbook::utils::fs::copy_files_except_ext;
use mdbook::{BookItem, book::Chapter};
use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap; // Used because it keeps things sorted alphabetically
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
    pub css_paths: Vec<String>,
    pub nav_separator: String,
    pub index: Option<String>,
    pub extra_nav: BTreeMap<String, String>,
    pub nav_bottom_empty: bool,
}

impl Default for TinyConfig {
    fn default() -> Self {
        Self {
            css_paths: Default::default(),
            nav_separator: " - ".to_string(),
            index: None,
            nav_bottom_empty: true,
            extra_nav: Default::default(),
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

    // test if the cfg works
    for nav in &cfg.extra_nav {
        println!("{} => {}", nav.0, nav.1)
    }

    let _ = fs::create_dir_all(&ctx.destination);

    // Copy over other files
    let _ = copy_files_except_ext(&ctx.source_dir(), &ctx.destination, true, None, &["md"]);

    for item in ctx.book.iter() {
        if let BookItem::Chapter(ref ch) = *item {
            if let Some(path) = &ch.path {
                // Write to a file
                let depth = (path.components().count()) as u8;

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
    // Ideally: configurable

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

    let css_content = &cfg
        .css_paths
        .iter()
        .map(|v| {
            format!(
                "<style>{}</style>",
                fs::read_to_string(ctx.source_dir().join(v)).unwrap()
            )
        })
        .collect::<Vec<String>>()
        .join("");

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

    let book_title = &ctx.config.book.title;
    let title_html = match book_title {
        Some(t) => format!("<h1>{t}</h1>"),
        None => "".to_string(),
    };
    let nav = nav_links(ctx, cfg, ch, depth).join(&cfg.nav_separator);
    let header = format!("{title_html}{nav}<hr>");
    let _ = writer.write(
        format!("<!doctype html>\n<head>{title_head}{description_head}<meta charset=\"utf8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">{css_content}</head><body><header>{header}</header><main>")
            .as_bytes(),
    );

    let events = highlight(parser)
        .unwrap()
        .into_iter()
        .map(|event| adjust_links(event, None));

    enum State<'a> {
        Default,
        InHeading {
            level: pulldown_cmark::HeadingLevel,
            classes: Vec<CowStr<'a>>,
            attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
            buffer: Vec<Event<'a>>,
        },
    }

    let mut state = State::Default;
    let mut new_events: Vec<Event> = Vec::new();

    for event in events {
        match state {
            State::Default => {
                if let Event::Start(Tag::Heading {
                    level,
                    id: _,
                    classes,
                    attrs,
                }) = event
                {
                    state = State::InHeading {
                        level,
                        classes,
                        attrs,
                        buffer: vec![],
                    };
                } else {
                    new_events.push(event);
                }
            }
            State::InHeading {
                level,
                classes,
                attrs,
                mut buffer,
            } => {
                if let Event::End(TagEnd::Heading(..)) = event {
                    let mut heading_text = String::new();
                    for ev in &buffer {
                        if let Event::Text(text) | Event::Code(text) = ev {
                            heading_text.push_str(text);
                        }
                    }

                    // transform to hyphenated lowercase, cutting out any suspicious symbols
                    let slug: String = heading_text
                        .to_lowercase()
                        .chars()
                        .filter_map(|c| match c {
                            ' ' => Some('-'),
                            c if c.is_numeric() || c.is_alphabetic() => Some(c),
                            _ => None,
                        })
                        .collect();

                    let start_heading = Event::Start(Tag::Heading {
                        level,
                        id: Some(slug.clone().into()),
                        classes,
                        attrs,
                    });
                    new_events.push(start_heading);

                    let link_url = format!("#{}", slug);
                    let link_start = Event::Start(Tag::Link {
                        link_type: LinkType::Inline,
                        dest_url: link_url.into(),
                        title: "".into(),
                        id: "".into(),
                    });
                    new_events.push(link_start);

                    new_events.append(&mut buffer);

                    let link_end = Event::End(TagEnd::Link);
                    new_events.push(link_end);

                    new_events.push(event); // End(Heading)
                    state = State::Default;
                } else {
                    buffer.push(event);
                    state = State::InHeading {
                        level,
                        classes,
                        attrs,
                        buffer,
                    };
                }
            }
        }
    }

    if let State::InHeading {
        level,
        classes,
        attrs,
        mut buffer,
    } = state
    {
        // Open heading, just dump it
        let start_heading = Event::Start(Tag::Heading {
            level,
            id: None,
            classes,
            attrs,
        });
        new_events.push(start_heading);
        new_events.append(&mut buffer);
    }

    let _ = pulldown_cmark::html::write_html_io(&mut writer, new_events.into_iter());

    let bottom_nav = child_links(ctx, cfg, ch, depth).join("");
    let _ = writer.write(format!("</main><footer>{bottom_nav}</footer></body></html>").as_bytes());
}

fn nav_links(ctx: &RenderContext, cfg: &TinyConfig, ch: &Chapter, depth: u8) -> Vec<String> {
    let mut links: Vec<String> = vec![format!(
        "<a href=\"{}\">Home</a>",
        apply_depth("".to_string(), depth)
    )];
    for nav in &cfg.extra_nav {
        // separate full urls from internal paths
        if nav.1.starts_with("http://") || nav.1.starts_with("https://") {
            links.push(format!("<a href=\"{}\">{}</a>", nav.1.to_string(), nav.0))
        } else {
            links.push(format!(
                "<a href=\"{}\">{}</a>",
                apply_depth(nav.1.to_string().replace(".md", ""), depth),
                nav.0
            ))
        }
    }
    let parents = &ch.parent_names;
    ctx.book.sections.iter().for_each(|sec| match sec {
        BookItem::Chapter(ich) => {
            if parents.iter().any(|p| p.eq(&ich.name)) {
                if let Some(path) = ich.path.clone() {
                    let link = format!(
                        "<a href=\"{}\">{}</a>",
                        apply_depth(path.to_str().unwrap_or("").replace(".md", ""), depth),
                        ich.name
                    );
                    // avoid exact duplicates
                    if !links.contains(&link) {
                        links.push(link)
                    }
                }
            }
        }
        _ => {}
    });
    return links;
}

fn child_links(ctx: &RenderContext, cfg: &TinyConfig, ch: &Chapter, depth: u8) -> Vec<String> {
    let mut links: Vec<String> = vec![];
    let children = &ch.sub_items;
    if children.len() == 0 {
        return match cfg.nav_bottom_empty {
            true => links,
            false => nav_links(ctx, cfg, ch, depth),
        };
    }
    children.iter().for_each(|item| match item {
        BookItem::Chapter(ich) => {
            links.push("<ul>".to_string());
            if let Some(path) = ich.path.clone() {
                links.push(format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    apply_depth(path.to_str().unwrap_or("").replace(".md", ""), depth),
                    ich.name,
                ));
                child_links(ctx, cfg, ich, depth)
                    .iter()
                    .for_each(|child| links.push(child.to_string()));
            }
            links.push("</ul>".to_string());
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
fn custom_parser(input: &str) -> Parser<'_> {
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
