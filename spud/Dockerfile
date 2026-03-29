FROM rust:bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM spudkit-base
RUN apt-get update && apt-get install -y git libgit2-1.5 curl jq && rm -rf /var/lib/apt/lists/*
RUN git config --global user.name "blogtato" && git config --global user.email "blogtato@localhost"
RUN curl -sL "https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js" -o /app/gui/htmx.min.js
COPY --from=builder /src/target/release/blog /usr/local/bin/blog
COPY spud/app/ /app/
RUN chmod +x /app/bin/*.sh

LABEL io.github.kantord.spudkit.shared_app_data="blogtato"
