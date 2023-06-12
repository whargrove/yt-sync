FROM rust:1.70 as builder
WORKDIR /usr/src/app
COPY ./src /usr/src/app/src
COPY ./Cargo.toml /usr/src/app/Cargo.toml
COPY ./Cargo.lock /usr/src/app/Cargo.lock
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install curl python3 -y
RUN curl -Ss -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp
COPY --from=builder /usr/src/app/target/release/yt-sync /usr/local/bin/yt-sync
WORKDIR /app
RUN mkdir /app/archives
RUN mkdir /app/channels
RUN touch /app/channels.json
VOLUME [ "/app/archives", "/app/channels", "/app/channels.json" ]
CMD ["yt-sync"]
