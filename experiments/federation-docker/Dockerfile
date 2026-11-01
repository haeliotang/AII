# Two-container partition preview node. Build from the repository root:
#   docker build -f experiments/federation-docker/Dockerfile -t poie-federation-docker .
FROM rust:1.94-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p poie-federation-docker

FROM debian:bookworm-slim
COPY --from=build /src/target/release/poie-federation-docker /usr/local/bin/poie-federation-docker
ENTRYPOINT ["poie-federation-docker"]
