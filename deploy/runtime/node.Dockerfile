# CI 运行时镜像：二进制由自建 runner 宿主编译（ubuntu 24.04），这里只做组装。
FROM ubuntu:24.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY su8-egress-node /usr/local/bin/su8-egress-node

EXPOSE 443
ENTRYPOINT ["/usr/local/bin/su8-egress-node", "--config", "/etc/su8-egress/config.toml"]
