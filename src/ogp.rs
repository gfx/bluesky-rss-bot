use html5ever::{
    tendril::StrTendril,
    tokenizer::{BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer},
    Attribute,

    // "content"
    ATOM_LOCALNAME__63_6F_6E_74_65_6E_74 as NAME_CONTENT,
    // "meta"
    ATOM_LOCALNAME__6D_65_74_61 as NAME_META,
    // "property"
    ATOM_LOCALNAME__70_72_6F_70_65_72_74_79 as NAME_PROPERTY,
};

#[derive(Default, Debug, PartialEq)]
pub struct Ogp {
    pub og_image: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_url: Option<String>,
}

// find a `content` attribute that has a specified `property` attribute,
// e.g. find_property_content("og:image") will return the `content` (i.e. url) of <meta property="og:image" content="...">
fn find_property_content(attrs: &Vec<Attribute>, property: &str) -> Option<String> {
    if attrs
        .iter()
        .find(|a| a.name.local == NAME_PROPERTY && a.value.to_lowercase() == property)
        .is_some()
    {
        // then get the value of the `content` attribute
        if let Some(content) = attrs
            .iter()
            .find(|a| a.name.local == NAME_CONTENT)
            .map(|a| a.value.to_string())
        {
            return Some(content);
        }
    }
    None
}

impl TokenSink for Ogp {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(Tag {
                kind: TagKind::StartTag,
                name,
                self_closing: _,
                attrs,
            }) => match name {
                NAME_META => {
                    if let Some(og_title) = find_property_content(&attrs, "og:title") {
                        self.og_title = Some(og_title);
                    }
                    if let Some(og_description) = find_property_content(&attrs, "og:description") {
                        self.og_description = Some(og_description);
                    }
                    if let Some(og_url) = find_property_content(&attrs, "og:url") {
                        self.og_url = Some(og_url);
                    }
                    if let Some(og_image) = find_property_content(&attrs, "og:image") {
                        self.og_image = Some(og_image);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

pub fn get_ogp(html: String) -> Ogp {
    let mut input = BufferQueue::new();
    if let Ok(tendril) = StrTendril::from_slice(&html).try_reinterpret() {
        input.push_back(tendril);
    } else {
        return Ogp::default();
    }
    let mut tok = Tokenizer::<Ogp>::new(Default::default(), Default::default());
    let _ = tok.feed(&mut input);
    tok.end();
    tok.sink
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ogp() {
        let body = r#"
        <html>
        <head>
        <meta property="og:title" content="Example Title" />
        <meta property="og:description" content="Example Description" />
        <meta property="og:url" content="https://example.com/" />
        <meta property="og:image" content="https://example.com/hero.png" />
        </head>
        <body>
        </body>
        </html>
        "#;

        let ogp = get_ogp(body.to_string());
        assert_eq!(ogp.og_title, Some("Example Title".to_string()));
        assert_eq!(ogp.og_description, Some("Example Description".to_string()));
        assert_eq!(ogp.og_url, Some("https://example.com/".to_string()));
        assert_eq!(
            ogp.og_image,
            Some("https://example.com/hero.png".to_string())
        );
    }

    #[test]
    fn test_get_ogp_empty() {
        let body = r#"
        <html>
        <head>
        </head>
        <body>
        </body>
        </html>
        "#;

        let ogp = get_ogp(body.to_string());
        assert_eq!(ogp.og_title, None);
        assert_eq!(ogp.og_description, None);
        assert_eq!(ogp.og_url, None);
        assert_eq!(ogp.og_image, None);
    }
}
