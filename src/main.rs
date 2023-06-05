use serde::Deserialize;
use std::error::Error;
use std::fs;

use youtube_dl::YoutubeDl;
use youtube_dl::YoutubeDlOutput::{Playlist, SingleVideo};

#[derive(Debug, Deserialize)]
struct Channel {
    url: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Read the channel URLs from a JSON file
    let channel_file = fs::read_to_string("channels.json")?;
    let channels: Vec<Channel> = serde_json::from_str(&channel_file)?;

    // TODO use parallelism / concurrency to take advantage of multiple cores
    // Iterate through the channels
    for channel in channels {
        // Extract the channel ID from the URL
        let channel_id = channel.url.rsplit('/').next().unwrap();

        // create an archive file if one doesn't already exist
        let archive_file = format!("archives/{}/archive.txt", channel_id);
        if !std::path::Path::new(&archive_file).exists() {
            fs::create_dir_all(format!("archives/{}", channel_id))?;
            fs::write(&archive_file, "")?;
        }

        let yt = YoutubeDl::new(&channel.url).run()?;
        match yt {
            SingleVideo(video) => println!("Video: {}", video.title),
            Playlist(playlist) => {
                println!("Downloaded playlist: {:?}", playlist.title);
                println!(
                    "Videos in playlist: {:?}",
                    playlist.entries.as_ref().map_or(0, |e| e.len())
                );
                if let Some(entries) = playlist.entries {
                    for entry in entries {
                        println!("Video: {}", entry.title);
                    }
                };
            }
        };

        // let command = format!("yt-dlp -f mp4 {} -o 'channels/%(channel)s/%(title)s.mp4' --download-archive 'archives/{}/archive.txt'", channel.url, channel_id);

        // // Execute the yt-dlp command
        // let _output = std::process::Command::new("sh")
        //     .arg("-c")
        //     .arg(&command)
        //     .stderr(Stdio::inherit())
        //     .stdout(Stdio::inherit())
        //     .output()?;
    }

    Ok(())
}
