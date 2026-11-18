# nrzVPS-Image (linux/amd64 + linux/arm64). Programm und Weboberfläche werden vorher auf dem Host
# gebaut (scripts/release.sh: cargo-zigbuild + dx) – hier wird nur gepackt: kein Compiler, kleines Image.
FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

# git: Apps aus Git-Repositories (nrzGit, GitHub …) klonen
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git tzdata \
 && rm -rf /var/lib/apt/lists/*

ARG TARGETARCH
COPY --chmod=755 dist/nrz-vps-${TARGETARCH} /usr/local/bin/nrz-vps
COPY dist/public /app/public
COPY llm.txt /app/llm.txt

ENV NRZ_VPS_BIND=0.0.0.0:8080 \
    NRZ_VPS_DATA=/data \
    NRZ_VPS_WEB=/app/public \
    NRZ_VPS_DOCKER=/var/run/docker.sock \
    RUST_LOG=info,tower_http=warn

VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["nrz-vps", "healthcheck"]
CMD ["nrz-vps"]
