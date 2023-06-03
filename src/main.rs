use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use std::error::Error;
use std::fs;
use std::process::Stdio;

#[derive(Debug, Deserialize)]
struct Channel {
    url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Read the channel URLs from a JSON file
    let channel_file = fs::read_to_string("channels.json")?;
    let channels: Vec<Channel> = serde_json::from_str(&channel_file)?;

    // Set up the SQLite database connection pool
    let pool = SqlitePoolOptions::new()
        .connect("sqlite:sync_history.db")
        .await?;

    // TODO use parallelism / concurrency to take advantage of multiple cores
    // Iterate through the channels
    for channel in channels {
        // Extract the channel ID from the URL
        let channel_id = channel.url.rsplit('/').next().unwrap();

        // Get the last synced date for the channel from the database
        let last_synced_date: Option<String> = sqlx::query_scalar!(
            r#"SELECT last_synced_date FROM sync_history WHERE channel_id = ?"#,
            channel_id
        )
        .fetch_optional(&pool)
        .await?
        .flatten();

        // Build the yt-dlp command
        let command = if let Some(last_date) = last_synced_date {
            format!(
                "yt-dlp -f mp4 {} --dateafter {} -o '%(channel)s/%(title)s.mp4'",
                channel.url, last_date
            )
        } else {
            format!("yt-dlp -f mp4 {} -o '%(channel)s/%(title)s.mp4'", channel.url)
        };

        // Execute the yt-dlp command
        let _output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()?;

        // Get the current date and time
        let current_date: DateTime<Utc> = Utc::now();

        // Update the last synced date in the database
        let current_date_rfc3339 = current_date.format("%Y%m%d").to_string();
        sqlx::query!(
            r#"INSERT INTO sync_history (channel_id, last_synced_date) VALUES (?, ?)
               ON CONFLICT(channel_id) DO UPDATE SET last_synced_date = ?"#,
            channel_id,
            current_date_rfc3339,
            current_date_rfc3339
        )
        .execute(&pool)
        .await?;
    }

    Ok(())
}
