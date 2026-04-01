use std::io::{self, Cursor};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesStart, BytesText, Event};

use crate::data::BlogData;

pub(crate) fn cmd_feed_export(store: &BlogData) -> anyhow::Result<()> {
    let feeds: Vec<_> = store.feeds().iter().map(|(_, f)| f).collect();
    anyhow::ensure!(!feeds.is_empty(), "No feeds to export");

    let mut buf = Cursor::new(Vec::new());
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    let mut opml = BytesStart::new("opml");
    opml.push_attribute(("version", "2.0"));
    writer.write_event(Event::Start(opml))?;

    writer.write_event(Event::Start(BytesStart::new("head")))?;
    writer.write_event(Event::Start(BytesStart::new("title")))?;
    writer.write_event(Event::Text(BytesText::new("blogtato feeds")))?;
    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("title")))?;
    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("head")))?;

    writer.write_event(Event::Start(BytesStart::new("body")))?;
    for feed in &feeds {
        let mut outline = BytesStart::new("outline");
        outline.push_attribute(("type", "rss"));
        outline.push_attribute(("text", feed.title.as_str()));
        outline.push_attribute(("title", feed.title.as_str()));
        outline.push_attribute(("xmlUrl", feed.url.as_str()));
        outline.push_attribute(("htmlUrl", feed.site_url.as_str()));
        writer.write_event(Event::Empty(outline))?;
    }
    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("body")))?;

    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("opml")))?;

    let xml = String::from_utf8(buf.into_inner())?;
    io::Write::write_all(&mut io::stdout(), xml.as_bytes())?;
    println!();
    Ok(())
}
