use std::convert::From;
use std::fmt::Display;
use std::ops::Range;
use std::slice;
use std::str::from_utf8_unchecked;
use std::string::String;

extern crate thin_http;
use thin_http::wininet;

extern crate epub_builder;
use epub_builder::EpubBuilder;
use epub_builder::EpubContent;
use epub_builder::ReferenceType;
use epub_builder::ZipLibrary;

fn id_to_url(id: u32) -> String {
    format!("http://syosetu.org/?mode=ss_view_all&nid={}", id)
}

fn view_all(id: u32) -> Option<String> {
    let url = id_to_url(id);
    let internet = wininet::Internet::open("agent", None)?;
    let response = internet.get(&url, None)?;
    assert_eq!(response.status(), 200);
    match response.body_as_string() {
        Ok(s) => Some(s),
        _ => None,
    }
}

unsafe fn range_to_str<'a>(range: Range<*const u8>) -> &'a str {
    let Range { start, end } = range;
    from_utf8_unchecked(slice::from_raw_parts(
        start,
        end.offset_from(start) as usize,
    ))
}

trait TextUtility {
    fn between<'a>(self: &'a Self, start: &str, end: &str) -> Option<(&'a str, &'a str)>;
    fn skip_until<'a>(self: &'a Self, t: &str) -> Option<&'a str>;
    fn skip_while<'a>(self: &'a Self, p: impl Fn(char) -> bool) -> &'a str;
}

impl TextUtility for str {
    fn between<'a>(self: &'a Self, start: &str, end: &str) -> Option<(&'a str, &'a str)> {
        let target_start = self.matches(start).next()?.as_bytes().as_ptr_range().end;
        let base_end = self.as_bytes().as_ptr_range().end;
        let target = unsafe { range_to_str::<'a>(target_start..base_end) };
        let Range {
            start: target_end,
            end: rest_start,
        } = target.matches(end).next()?.as_bytes().as_ptr_range();
        Some((
            unsafe { range_to_str::<'a>(target_start..target_end) },
            unsafe { range_to_str::<'a>(rest_start..base_end) },
        ))
    }

    fn skip_until<'a>(self: &'a str, t: &str) -> Option<&'a str> {
        let range_start = self.matches(t).next()?.as_bytes().as_ptr_range().end;
        let range_end = self.as_bytes().as_ptr_range().end;
        Some(unsafe { range_to_str::<'a>(range_start..range_end) })
    }

    fn skip_while<'a>(self: &'a str, p: impl Fn(char) -> bool) -> &'a str {
        let mut iter = self.chars();
        while let Some(c) = iter.next() {
            if !p(c) {
                break;
            }
        }
        iter.as_str()
    }
}

struct Episode<'a> {
    title: &'a str,
    body: &'a str,
}

struct Novel<'a> {
    title: &'a str,
    author: &'a str,
    episodes: Vec<Episode<'a>>,
}

impl<'a> Novel<'a> {
    fn to_epub_builder(&self) -> epub_builder::Result<EpubBuilder<ZipLibrary>> {
        let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;
        builder.metadata("author", self.author)?;
        builder.metadata("title", self.title)?;
        for (num, x) in self.episodes.iter().enumerate() {
            builder.add_content(
                EpubContent::new(format!("{:<}.html", num), x.body.as_bytes())
                    .title(x.title)
                    .reftype(ReferenceType::Text),
            )?;
        }
        builder.inline_toc();
        Ok(builder)
    }

    fn make_filename(&self) -> String {
        format!("[{}] {}.epub", sanitize(self.author), sanitize(self.title))
    }
}

impl<'a> Episode<'a> {
    fn new(title: &'a str, body: &'a str) -> Episode<'a> {
        Episode { title, body }
    }
}

impl<'a> From<(&'a str, &'a str)> for Episode<'a> {
    fn from(t: (&'a str, &'a str)) -> Self {
        Episode::new(t.0, t.1)
    }
}

fn find_episode<'a>(s: &'a str) -> Option<(Episode, &'a str)> {
    let (title, rest) = s.between("<span style=\"font-size:large\">", "</span>")?;
    let (body, rest) = rest.between("<div class=\"honbun\">", "</div>\n")?;
    Some((Episode::new(title, body), rest))
}

struct EpisodeFinder<'a>(&'a str);

impl<'a> EpisodeFinder<'a> {
    fn new(s: &'a str) -> EpisodeFinder<'a> {
        EpisodeFinder(s)
    }
}

impl<'a> Iterator for EpisodeFinder<'a> {
    type Item = Episode<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let (value, rest) = find_episode(self.0)?;
        self.0 = rest;
        Some(value)
    }
}

trait Scrape {
    fn scrape(self: &Self) -> std::result::Result<Novel, &'static str>;
}

impl Scrape for str {
    fn scrape(self: &Self) -> std::result::Result<Novel, &'static str> {
        let (title, rest) = self.between("<title>", "</title>").ok_or("Failed to get title.")?;
        let rest = rest.skip_until("<a href=//syosetu.org/user/").ok_or("failed to search author.")?;
        let rest = rest.skip_while(|x| x.is_digit(10));
        let (author, rest) = rest.between(">", "</a>").ok_or("Failed to get author")?;
        let episodes = EpisodeFinder::new(rest).collect::<Vec<_>>();
        Ok(Novel {
            title,
            author,
            episodes,
        })
    }
}

impl<'a> Display for Episode<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, include_str!("html.template"), self.title, self.body)
    }
}

use std::fs::File;
use std::iter::Iterator;

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|ch| {
            if let Some(_) = "\\/:*?\"<>|".find(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>()
}

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "hameln-publish")]
struct Opt {
    #[structopt(name = "IDS")]
    ids: Vec<u32>,
}

fn main() -> std::result::Result<(), &'static str> {
    let opt = Opt::from_args();

    for id in opt.ids {
        let novel_text = view_all(id).ok_or("Faild to get novel data.")?;
        let novel_data = novel_text.scrape()?;

        let mut builder = novel_data.to_epub_builder().expect("Fail to build epub.");
        let mut file = File::create(novel_data.make_filename()).expect("Fail to open file.");
        builder
            .generate(&mut file)
            .expect("Fail to generate epub file.");
    }
    Ok(())
}
