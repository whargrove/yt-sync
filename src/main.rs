use serde::Deserialize;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use tokio::sync::mpsc;

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

// #[tokio::main(flavor = "multi_thread", worker_threads = 8)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Read the channel URLs from a JSON file
    let channel_file = fs::read_to_string("channels.json")?;
    let channels: Vec<Channel> = serde_json::from_str(&channel_file)?;
    let (tx, mut rx) = mpsc::channel::<ChannelVideoMessage>(16);
    for channel in channels {
        let cvm_tx = tx.clone();
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
                cvm_tx.send(m).await.unwrap();
            }
            drop(cvm_tx);
        });
    }

    drop(tx);

    // todo create workers that will listen for message produced from each channel
    // limit the number of workers to 8

    let mut handles = vec![];
    while let Some(cvm) = rx.recv().await {
        let handle = tokio::spawn(async move {
            download_video(cvm).await.unwrap();
        });
        handles.push(handle);
    }
    futures::future::join_all(handles).await;

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

async fn download_video(cvm: ChannelVideoMessage) -> Result<(), Box<dyn Error>> {
    if let Some(video_url) = cvm.video.url {
        YoutubeDl::new(video_url)
            .format("mp4")
            .download(true)
            .output_template("channels/%(channel)s/%(title)s.mp4")
            .extra_arg("--download-archive")
            .extra_arg(make_archive_file_path(cvm.channel_id.as_str()))
            .run_async()
            .await?;
    }
    Ok(())
}
