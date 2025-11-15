FROM rust:alpine3.22 AS builder

RUN apk add --no-cache \
    make \
    musl-dev \
    libressl-dev

COPY . .

RUN make dependencies
RUN make build


FROM alpine:3.22

COPY --from=builder /target/release/wakezilla /usr/local/bin/wakezilla

RUN mkdir -p /opt/wakezilla
ENV WAKEZILLA__STORAGE__MACHINES_DB_PATH=/opt/wakezilla/machines.json

WORKDIR /opt/wakezilla

# Exposes the default port for web application.
# To use port forwarding you need to add `--network host` to your `docker run` command.
EXPOSE 3000

ENTRYPOINT ["wakezilla"]
CMD ["proxy-server"]
