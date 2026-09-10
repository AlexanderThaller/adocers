//! Image, video and audio blocks.

use std::fmt::Write as _;

use asciidoc_parser::{
    attributes::Attrlist,
    blocks::{
        Block,
        MediaBlock,
        MediaType,
    },
};

use crate::render::{
    Renderer,
    html::escape_attr,
};

impl<'src> Renderer<'src> {
    /// Render `image::`, `video::` or `audio::`.
    pub(super) fn media_block(&mut self, block: &'src Block<'src>, media: &'src MediaBlock<'src>) {
        match media.type_() {
            MediaType::Image => self.image_block(block, media),
            MediaType::Video => self.video_block(block, media),
            MediaType::Audio => self.audio_block(block, media),
        }
    }

    /// `image::target[alt,width,height]`.
    fn image_block(&mut self, block: &'src Block<'src>, media: &'src MediaBlock<'src>) {
        let attrlist = media.macro_attrlist();
        let target = media.resolved_target();

        let alt =
            positional(attrlist, "alt", 1).map_or_else(|| alt_from_target(target), str::to_string);

        let mut img = format!(
            "<img src=\"{}\" alt=\"{}\"",
            escape_attr(target),
            escape_attr(&alt)
        );

        attribute(&mut img, "width", positional(attrlist, "width", 2));
        attribute(&mut img, "height", positional(attrlist, "height", 3));

        img.push('>');

        // `link=` makes the image itself the link target.
        if let Some(link) = named(attrlist, "link") {
            img = format!(
                "<a class=\"image\" href=\"{}\">{img}</a>",
                escape_attr(link)
            );
        }

        self.open_wrapper(block, "imageblock");
        self.out.open("div", None, &["content"]);
        self.out.line(&img);
        self.out.close("div");

        // An image's caption sits below the figure, unlike every other block.
        self.block_title(block);
        self.out.close("div");
    }

    /// `video::target[]`.
    fn video_block(&mut self, block: &'src Block<'src>, media: &'src MediaBlock<'src>) {
        let attrlist = media.macro_attrlist();

        let mut tag = format!("<video src=\"{}\"", escape_attr(media.resolved_target()));

        attribute(&mut tag, "width", named(attrlist, "width"));
        attribute(&mut tag, "height", named(attrlist, "height"));
        attribute(&mut tag, "poster", named(attrlist, "poster"));

        if !attrlist.has_option("nocontrols") {
            tag.push_str(" controls");
        }

        if attrlist.has_option("autoplay") {
            tag.push_str(" autoplay");
        }

        if attrlist.has_option("loop") {
            tag.push_str(" loop");
        }

        tag.push('>');

        self.open_wrapper(block, "videoblock");
        self.block_title(block);
        self.out.open("div", None, &["content"]);
        self.out.line(&tag);
        self.out
            .line("Your browser does not support the video tag.");
        self.out.line("</video>");
        self.out.close("div");
        self.out.close("div");
    }

    /// `audio::target[]`.
    fn audio_block(&mut self, block: &'src Block<'src>, media: &'src MediaBlock<'src>) {
        let attrlist = media.macro_attrlist();

        let mut tag = format!("<audio src=\"{}\"", escape_attr(media.resolved_target()));

        if !attrlist.has_option("nocontrols") {
            tag.push_str(" controls");
        }

        if attrlist.has_option("autoplay") {
            tag.push_str(" autoplay");
        }

        if attrlist.has_option("loop") {
            tag.push_str(" loop");
        }

        tag.push('>');

        self.open_wrapper(block, "audioblock");
        self.block_title(block);
        self.out.open("div", None, &["content"]);
        self.out.line(&tag);
        self.out
            .line("Your browser does not support the audio tag.");
        self.out.line("</audio>");
        self.out.close("div");
        self.out.close("div");
    }
}

/// Append ` name="value"` to a tag under construction, if there is a value.
fn attribute(tag: &mut String, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        // Writing to a `String` cannot fail.
        let _ = write!(tag, " {name}=\"{}\"", escape_attr(value));
    }
}

/// A macro attribute that may be given either by name or by position.
fn positional<'src>(attrlist: &'src Attrlist<'src>, name: &str, index: usize) -> Option<&'src str> {
    named(attrlist, name).or_else(|| {
        attrlist
            .nth_attribute(index)
            .filter(|attribute| attribute.name().is_none())
            .map(asciidoc_parser::attributes::ElementAttribute::value)
            .filter(|value| !value.is_empty())
    })
}

/// A named macro attribute, if it has a non-empty value.
fn named<'src>(attrlist: &'src Attrlist<'src>, name: &str) -> Option<&'src str> {
    attrlist
        .named_attribute(name)
        .map(asciidoc_parser::attributes::ElementAttribute::value)
        .filter(|value| !value.is_empty())
}

/// The alt text to use when the author gave none.
///
/// Asciidoctor falls back to the target's file name with its extension dropped
/// and separators turned into spaces, which is usually a better description
/// than an empty `alt` and keeps the image reachable to a screen reader.
fn alt_from_target(target: &str) -> String {
    let file_name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);

    stem.replace(['_', '-'], " ")
}
