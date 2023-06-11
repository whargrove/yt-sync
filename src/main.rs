extern crate crossbeam;

use crossbeam::channel::bounded;
use serde::Deserialize;
use std::error::Error;
use std::fmt::Display;
use std::fs;

use youtube_dl::YoutubeDlOutput::Playlist;
use youtube_dl::{SingleVideo, YoutubeDl};

#[derive(Debug, Deserialize)]
struct Channel {
    url: String,
}

#[derive(Debug)]
struct ChannelVideoMessage {
    channel_id: String,
    video: SingleVideo,
}

fn make_archive_file_path(channel_id: &str) -> String {
    format!("archives/{}/archive.txt", channel_id)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Read the channel URLs from a JSON file
    let channel_file = fs::read_to_string("channels.json")?;
    let channels: Vec<Channel> = serde_json::from_str(&channel_file)?;

    let (tx, rx) = bounded::<ChannelVideoMessage>(16);

    for channel in channels {
        let tx = tx.clone();
        tokio::spawn(async move {
            let messages = get_videos_from_channel(&channel).await.unwrap();
            if !messages.is_empty() {
                let channel_id = get_channel_id(&channel).unwrap();
                let archive_file = make_archive_file_path(channel_id);
                if !std::path::Path::new(&archive_file).exists() {
                    fs::create_dir_all(format!("archives/{}", channel_id)).unwrap();
                    fs::write(&archive_file, "").unwrap();
                }
            }
            for m in messages {
                tx.send(m).unwrap();
            }
            drop(tx);
        });
    }

    drop(tx);

    let workers = num_cpus::get();
    for _ in 0..workers {
        let rx = rx.clone();
        tokio::spawn(async move {
            for m in rx.iter() {
                // TODO Error handling
                download_video(m).unwrap();
            }
        });
    }

    Ok(())
}

#[derive(Debug)]
struct MalformedChannelUrlError {
    bad_url: String,
}

impl Error for MalformedChannelUrlError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn Error> {
        self.source()
    }
}

impl Display for MalformedChannelUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not a valid channel URL", self.bad_url)
    }
}

fn get_channel_id(channel: &Channel) -> Result<&str, MalformedChannelUrlError> {
    channel
        .url
        .rsplit('/')
        .next()
        .ok_or(MalformedChannelUrlError {
            bad_url: channel.url.clone(),
        })
}

async fn get_videos_from_channel(
    channel: &Channel,
) -> Result<Vec<ChannelVideoMessage>, Box<dyn Error>> {
    let channel_id = get_channel_id(channel)?;
    let videos = match YoutubeDl::new(format!("{}/videos", channel.url))
        .flat_playlist(true)
        .run_async()
        .await?
    {
        Playlist(playlist) => {
            if let Some(single_videos) = playlist.entries {
                single_videos
                    .into_iter()
                    .map(|single_video| ChannelVideoMessage {
                        channel_id: channel_id.to_string(),
                        video: single_video,
                    })
                    .collect::<Vec<ChannelVideoMessage>>()
            } else {
                vec![]
            }
        }
        _ => vec![],
    };

    Ok(videos)
}

fn download_video(cvm: ChannelVideoMessage) -> Result<(), Box<dyn Error>> {
    if let Some(video_url) = cvm.video.url {
        YoutubeDl::new(video_url)
            .format("mp4")
            .download(true)
            // todo put the date of the video in the title
            .output_template("channels/%(channel)s/%(title)s.mp4")
            .extra_arg("--download-archive")
            .extra_arg(make_archive_file_path(cvm.channel_id.as_str()))
            .run()?;
    }
    Ok(())
}
