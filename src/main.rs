extern crate crossbeam;

use crossbeam::channel::unbounded;
use env_logger::Env;
use futures::future::join_all;
use log::{debug, info, warn};
use serde::Deserialize;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use tokio::task::JoinHandle;

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
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);

    // Read the channel URLs from a JSON file
    let channel_file = fs::read_to_string("channels.json")?;
    let channels: Vec<Channel> = serde_json::from_str(&channel_file)?;

    let (tx, rx) = unbounded::<ChannelVideoMessage>();
    let mut channel_handles: Vec<JoinHandle<()>> = Vec::with_capacity(channels.len());
    for channel in channels {
        info!("Spawning thread to get videos from {}.", channel.url);
        let channel_video_tx = tx.clone();
        channel_handles.push(tokio::spawn(async move {
            info!("Getting videos from {}.", channel.url);
            let messages = get_videos_from_channel(&channel).await.unwrap();
            info!("Got {} videos from {}.", messages.len(), channel.url);
            if !messages.is_empty() {
                let channel_id = get_channel_id(&channel).unwrap();
                let archive_file = make_archive_file_path(channel_id);
                if !std::path::Path::new(&archive_file).exists() {
                    info!("Creating archive file at {}.", archive_file);
                    fs::create_dir_all(format!("archives/{}", channel_id)).unwrap();
                    fs::write(&archive_file, "").unwrap();
                }
            }
            for m in messages {
                let video_id = m.video.id.clone();
                debug!("Sending video message {}.", m.video.id);
                match channel_video_tx.send(m) {
                    Ok(_) => debug!("Sent video message {}.", video_id),
                    Err(e) => warn!(
                        "Failed to send video message {} from channel {}. Error: {}",
                        video_id, channel.url, e
                    ),
                };
            }
            drop(channel_video_tx);
        }));
    }

    drop(tx);
    join_all(channel_handles).await;

    let workers = num_cpus::get();
    info!("Spawning {} worker threads.", workers);
    let mut worker_handles: Vec<JoinHandle<()>> = Vec::with_capacity(workers);
    for worker_id in 0..workers {
        let worker_rx = rx.clone();
        worker_handles.push(tokio::spawn(async move {
            info!("Worker {} reporting for duty.", worker_id);
            for m in worker_rx.iter() {
                let video_id = m.video.id.clone();
                match download_video(m) {
                    Ok(_) => debug!("Worker {} downloaded video {}.", worker_id, video_id),
                    Err(e) => warn!(
                        "Worker {} failed to download video {}. Error: {}",
                        worker_id, video_id, e
                    ),
                };
            }
            info!("Worker {} 🫡  signing off.", worker_id);
        }));
    }

    join_all(worker_handles).await;

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
            info!("Found playlist for channel {}.", channel.url);
            if let Some(single_videos) = playlist.entries {
                info!(
                    "Found {} videos for channel {}.",
                    single_videos.len(),
                    channel.url
                );
                single_videos
                    .into_iter()
                    .map(|single_video| ChannelVideoMessage {
                        channel_id: channel_id.to_string(),
                        video: single_video,
                    })
                    .collect::<Vec<ChannelVideoMessage>>()
            } else {
                warn!("The playlist for channel {} has no videos.", channel.url);
                vec![]
            }
        }
        _ => {
            warn!("No videos found for channel {}.", channel.url);
            vec![]
        }
    };

    Ok(videos)
}

fn download_video(cvm: ChannelVideoMessage) -> Result<(), Box<dyn Error>> {
    if let Some(video_url) = cvm.video.url {
        info!("Downloading video {} from {}.", video_url, cvm.channel_id);
        return match YoutubeDl::new(video_url)
            .format("mp4")
            .download(true)
            // When viewing channels by directory in Plex the videos are sorted by file name.
            // Plex doesn't offer any other sorting options, so adding the upload date as a prefix
            // on the filename is an 80% solution here. Not great, but it works!
            .output_template("channels/%(channel)s/%(upload_date>%Y-%m-%d)s - %(title)s.mp4")
            .extra_arg("--download-archive")
            .extra_arg(make_archive_file_path(cvm.channel_id.as_str()))
            .run()
        {
            Ok(_) => {
                debug!(
                    "Successfully downloaded video {} from {}.",
                    cvm.video.id, cvm.channel_id
                );
                Ok(())
            }
            Err(e) => {
                return match e {
                    youtube_dl::Error::Json(_) => {
                        debug!(
                            "Video {} from {} has already been downloaded.",
                            cvm.video.id, cvm.channel_id
                        );
                        Ok(())
                    }
                    _ => Err(Box::new(e)),
                }
            }
        };
    } else {
        warn!(
            "Video {} from {} has no URL! Unable to download.",
            cvm.video.id, cvm.channel_id
        );
        Ok(())
    }
}
