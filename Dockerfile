FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y \
    make \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN make dependencies
RUN make build


FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /target/release/wakezilla /usr/local/bin/wakezilla

RUN mkdir -p /opt/wakezilla
ENV WAKEZILLA__STORAGE__MACHINES_DB_PATH=/opt/wakezilla/machines.json

WORKDIR /opt/wakezilla

EXPOSE 3000

ENTRYPOINT ["wakezilla"]
CMD ["proxy-server"]
