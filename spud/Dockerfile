FROM rust:bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM spudkit-base
RUN apt-get update && apt-get install -y git libgit2-1.5 curl jq && rm -rf /var/lib/apt/lists/*
RUN curl -sL "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css" -o /app/gui/pico.min.css
RUN curl -sL "https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js" -o /app/gui/htmx.min.js
COPY --from=builder /src/target/release/blog /usr/local/bin/blog
COPY spud/app/ /app/
RUN chmod +x /app/bin/*.sh

ENV RSS_STORE=/data/store

# Symlink so blog finds the store even without ENV
RUN mkdir -p /root/.local/share/blogtato/stores && \
    ln -s /data/store /root/.local/share/blogtato/stores/default

# Subscribe to some feeds and sync
RUN blog feed add https://drewdevault.com/blog/index.xml \
    && blog feed add https://simonwillison.net/atom/everything/ \
    && blog feed add https://tonsky.me/atom.xml \
    && blog feed add https://overreacted.io/rss.xml \
    && blog feed add https://acko.net/atom.xml \
    && blog sync
