// main.rs

use std::fs;
use std::io::{self, Write};
use std::process::Command;
use std::error::Error;
use rss::Channel;
use epub_builder::{EpubBuilder, ZipLibrary, EpubContent, ReferenceType};
use html2text::from_read;
use lettre::{Message, SmtpTransport, Transport};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use reqwest;
use scraper::{Html, Selector};
use serde::Deserialize;
use sanitize_html::sanitize_str;
use sanitize_html::rules::predefined::DEFAULT;

#[derive(Deserialize)]
struct Config {
    rss_feeds: Vec<String>,
    email: EmailConfig,
}

#[derive(Deserialize)]
struct EmailConfig {
    from: String,
    to: String,
    smtp_server: String,
    username: String,
    password: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Load configuration from YAML file
    let config: Config = serde_yaml::from_str(&fs::read_to_string("./config.yml")?)?;

    let mut epub_builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
    epub_builder.metadata("author", "RSS to EPUB Generator")?;
    epub_builder.metadata("title", "RSS Feed Compilation")?;

    for url in &config.rss_feeds {
        let channel = fetch_rss(url)?;
        for item in channel.items() {
            if let Some(title) = item.title() {
                let content = if let Some(content) = item.content() {
                    sanitize_html_content(&content)
                } else if let Some(link) = item.link() {
                    fetch_full_content(link)?
                } else {
                    sanitize_html_content("No content available")
                };
                let content = format!("<p>{}</p>",content);
                let title_sam= remove_invalid_characters_from_title(&title);
                let content_title = format!("aa{}.xhtml", title_sam.replace("/", "_").replace(" ", "_").replace("'","_"));
                epub_builder.add_content(
                    EpubContent::new(&content_title, content.as_bytes()).title(content_title.to_string()).reftype(ReferenceType::Text),
                )?;
            }
        }
    }

    // Write EPUB to file
    let mut epub_file = fs::File::create("rss_feed.epub")?;
    epub_builder.generate(&mut epub_file)?;

    // Send email with the EPUB
    send_epub_via_email("rss_feed.epub", &config.email)?;

    Ok(())
}

fn fetch_rss(url: &str) -> Result<Channel, Box<dyn Error>> {
    let content = reqwest::blocking::get(url)?.text()?;
    let channel = Channel::read_from(content.as_bytes())?;
    Ok(channel)
}

fn fetch_full_content(url: &str) -> Result<String, Box<dyn Error>> {
    let content = reqwest::blocking::get(url)?.text()?;
    let document = Html::parse_document(&content);

    let paragraph_selector = Selector::parse("p").unwrap();
    let anchor_selector = Selector::parse("a").unwrap();
    let mut full_content = String::new();

    for element in document.select(&paragraph_selector) {
        let mut paragraph_html = element.inner_html();

        // Remove hyperlinks (<a> tags) from the paragraph
        let fragment = Html::parse_fragment(&paragraph_html);
        for anchor in fragment.select(&anchor_selector) {
            if let Some(anchor_text) = anchor.text().next() {
                paragraph_html = paragraph_html.replace(&anchor.html(), anchor_text);
            }
        }

        full_content.push_str(&format!("<p>{}</p>", paragraph_html));
    }

    Ok(sanitize_html_content(&full_content))
}

fn sanitize_html_content(content: &str) -> String {
    //from_read(content.as_bytes(),50).unwrap()
    sanitize_str(&DEFAULT,&content).unwrap()
}

fn remove_invalid_characters_from_title(content: &str) -> String {
    content.chars().filter(
        |&c|c.is_ascii()
        &&c!=':'
        &&c!='?'
        &&c!='%'
    ).collect()
}


fn send_epub_via_email(file_path: &str, email_config: &EmailConfig) -> Result<(), Box<dyn Error>> {
    let email = Message::builder()
        .from(email_config.from.parse()?)
        .to(email_config.to.parse()?)
        .subject("Your RSS Feed Compilation")
        .header(ContentType::parse("application/epub+zip")?)
        .body(fs::read(file_path)?)?;

    let credentials = Credentials::new(email_config.username.clone(), email_config.password.clone());

    let mailer = SmtpTransport::relay(&email_config.smtp_server)?
        .credentials(credentials)
        .build();

    mailer.send(&email)?;

    Ok(())
}
