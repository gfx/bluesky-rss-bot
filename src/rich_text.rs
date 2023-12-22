// building facets is challenging!
// cf. https://github.com/bluesky-social/atproto/blob/main/packages/api/src/rich-text/rich-text.ts

use atrium_api::app::bsky::richtext::facet::{ByteSlice, Link, Main, MainFeaturesItem};

#[derive(Default, Debug)]
pub struct RichTextBuilder {
    text: String,
    facets: Vec<Main>,
}

// text() - add a plain text
// link() - add a link
impl RichTextBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn text<'a, S: Into<&'a str>>(mut self, text: S) -> Self {
        self.text.push_str(text.into());
        self
    }

    pub fn link<'a, S: Into<&'a str>>(mut self, url: S) -> Self {
        let s = url.into();

        self.facets.push(Main {
            features: vec![MainFeaturesItem::Link(Box::new(Link { uri: s.into() }))],
            index: ByteSlice {
                byte_start: self.text.len() as i32,
                byte_end: (self.text.len() + s.len()) as i32,
            },
        });
        self.text.push_str(s);
        self
    }

    pub fn build(self) -> (String, Vec<Main>) {
        (self.text, self.facets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rich_text_builder() {
        let (text, facets) = RichTextBuilder::new()
            .text("hello ")
            .link("https://example.com/")
            .text(" world")
            .build();

        assert_eq!(text, "hello https://example.com/ world");

        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].features.len(), 1);
        assert_eq!(
            facets[0].features[0],
            MainFeaturesItem::Link(Box::new(Link {
                uri: "https://example.com/".into()
            }))
        );
        assert_eq!(facets[0].index.byte_start, 6);
        assert_eq!(facets[0].index.byte_end, 26);
    }
}
