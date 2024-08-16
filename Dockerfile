FROM bitnami/minideb:bookworm

LABEL org.opencontainers.image.source=https://github.com/cpg314/ltapiserv-rs
LABEL org.opencontainers.image.licenses=MIT

COPY target-cross/x86_64-unknown-linux-gnu/release/ltapiserv-rs /usr/bin/ltapiserv-rs
COPY target-cross/x86_64-unknown-linux-gnu/release/ltapi-client /usr/bin/ltapi-client

EXPOSE 8875

VOLUME /data

ENTRYPOINT ["/usr/bin/ltapiserv-rs"]
CMD ["--dictionary", "/data/dictionary.txt"]
