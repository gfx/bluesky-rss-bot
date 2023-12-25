// This application does the following tasks:
// 1. Reads an RSS feed.
// 2. Reads the posts from a Bluesky account.
// 3. Posts the articles to Bluesky that the account doesn't have.

use atrium_api::{
    agent::store::{MemorySessionStore, SessionStore},
    agent::{AtpAgent, Session},
    app::bsky::feed::defs::{FeedViewPost, PostViewEmbedEnum},
    app::bsky::feed::post::RecordEmbedEnum,
    xrpc::XrpcClient,
};
use atrium_xrpc_client::reqwest::ReqwestClient;
use feed_rs::model::{Entry, Feed};
use unicode_normalization::UnicodeNormalization;

use bluesky_rss_bot::ogp::{get_ogp, Ogp};
use bluesky_rss_bot::rich_text::RichTextBuilder;

// Bluesky post endpoint.
// https://atproto.com/blog/create-post
const BSKY_API_DEFAULT_URL: &str = "https://bsky.social";

async fn get_feed(url: String) -> Result<Feed, Box<dyn std::error::Error>> {
    let resp = reqwest::get(&url).await?.bytes().await?;
    let feed = feed_rs::parser::parse_with_uri(resp.as_ref(), Some(&url))?;
    Ok(feed)
}

async fn get_bsky_posts<S: SessionStore + Send + Sync, T: XrpcClient + Send + Sync>(
    agent: &AtpAgent<S, T>,
    session: &Session,
) -> Result<Vec<FeedViewPost>, Box<dyn std::error::Error>> {
    let posts = agent
        .api
        .app
        .bsky
        .feed
        .get_author_feed(atrium_api::app::bsky::feed::get_author_feed::Parameters {
            actor: session.handle.clone(),
            filter: Some("posts_no_replies".into()),
            cursor: None,
            limit: Some(10),
        })
        .await?
        .feed;

    Ok(posts)
}

// Finds new entries in Feed that are not in posts.
async fn find_new_entries(
    session: &Session,
    feed: Feed,
    posts: Vec<FeedViewPost>,
) -> Result<Vec<Entry>, Box<dyn std::error::Error>> {
    let mut new_entries = vec![];

    for entry in feed.entries {
        let mut found = false;
        for post in &posts {
            if post.post.author.did != session.did {
                // ignore reposts
                continue;
            }

            if let Some(PostViewEmbedEnum::AppBskyEmbedExternalView(view)) = &post.post.embed {
                if view.external.uri == entry.id {
                    println!("Found a post with the same uri: {}", entry.id);
                    found = true;
                    break;
                }
            }
        }

        if found {
            break;
        } else {
            new_entries.push(entry);
        }
    }

    Ok(new_entries)
}

async fn get_ogp_from_url(url: &str) -> Result<Ogp, Box<dyn std::error::Error>> {
    let mut resp = reqwest::get(url).await;
    // retry once if get() fails or the status code is 50x.
    if resp.is_err() || resp.as_ref().unwrap().status().is_server_error() {
        resp = reqwest::get(url).await;
    }
    let content = resp?.text().await?;
    Ok(get_ogp(content))
}

async fn create_embed_ogp<S: SessionStore + Send + Sync, T: XrpcClient + Send + Sync>(
    agent: &AtpAgent<S, T>,
    source_url: &str,
) -> Result<RecordEmbedEnum, Box<dyn std::error::Error>> {
    let ogp = get_ogp_from_url(source_url).await?;
    let og_image = ogp
        .og_image
        .ok_or_else(|| format!("og:image is not found in {:?}", source_url))?;
    let blob = reqwest::get(og_image).await?.bytes().await?;
    let uploaded_blob = agent
        .api
        .com
        .atproto
        .repo
        .upload_blob(blob.into())
        .await?
        .blob;

    let uri = source_url.into();
    let title = ogp.og_title.unwrap_or("".into()).nfc().collect::<String>();
    let description = ogp.og_description.unwrap_or("".into());
    let thumb = Some(uploaded_blob);

    let embed = RecordEmbedEnum::AppBskyEmbedExternalMain(Box::new(
        atrium_api::app::bsky::embed::external::Main {
            external: atrium_api::app::bsky::embed::external::External {
                uri,
                title,
                description,
                thumb,
            },
        },
    ));

    Ok(embed)
}

async fn post_entry<S: SessionStore + Send + Sync, T: XrpcClient + Send + Sync>(
    agent: &AtpAgent<S, T>,
    session: &Session,
    entry: Entry,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = entry.id;
    let title = entry
        .title
        .as_ref()
        .unwrap()
        .content
        .nfc()
        .collect::<String>();

    let embed = create_embed_ogp(agent, &url).await.ok();

    // build the body of a post as a ATProtocol's rich text.
    let (text, facets) = RichTextBuilder::new()
        .text(&*title)
        .text("\n\n")
        .link(&*url)
        .build();

    println!("Posting entry {:?} {:?}", text, embed);

    let result = agent
        .api
        .com
        .atproto
        .repo
        .create_record(atrium_api::com::atproto::repo::create_record::Input {
            collection: "app.bsky.feed.post".into(),
            record: atrium_api::records::Record::AppBskyFeedPost(Box::new(
                atrium_api::app::bsky::feed::post::Record {
                    text,
                    embed,
                    facets: Some(facets),
                    created_at: chrono::Local::now().to_rfc3339(),
                    entities: None,
                    labels: None,
                    langs: None,
                    reply: None,
                    tags: None,
                },
            )),
            repo: session.handle.clone(),
            rkey: None,
            swap_commit: None,
            validate: None,
        })
        .await?;

    println!("Successfully posted an entry: {:?}", result.uri);

    Ok(())
}

async fn post_entries<S: SessionStore + Send + Sync, T: XrpcClient + Send + Sync>(
    agent: &AtpAgent<S, T>,
    session: &Session,
    entries: Vec<Entry>,
    max_bsky_posts: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for entry in entries.into_iter().rev() {
        if count > max_bsky_posts {
            break;
        }
        post_entry(agent, session, entry).await?;
        count += 1;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // credentials are passed via environment variables.
    let rss_url = std::env::var("RSS_URL")?;
    let bsky_id = std::env::var("BSKY_ID")?;
    let bsky_password = std::env::var("BSKY_PASSWORD")?;
    let bsky_api_url =
        std::env::var("BSKY_API_URL").unwrap_or_else(|_| BSKY_API_DEFAULT_URL.into());
    let max_bsky_posts = std::env::var("MAX_BSKY_POSTS")
        .unwrap_or("".into())
        .parse::<i32>()
        .unwrap_or(10);

    let bsky_agent = AtpAgent::new(
        ReqwestClient::new(bsky_api_url),
        MemorySessionStore::default(),
    );
    let session = bsky_agent.login(bsky_id, bsky_password).await?;
    println!(
        "Successfully logged in as {} ({})",
        session.handle, session.did
    );

    let feed = get_feed(rss_url).await?;
    let posts = get_bsky_posts(&bsky_agent, &session).await?;
    let new_entries = find_new_entries(&session, feed, posts).await?;

    println!("Found {} new entries", new_entries.len());

    post_entries(&bsky_agent, &session, new_entries, max_bsky_posts).await?;

    Ok(())
}
