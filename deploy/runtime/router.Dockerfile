# CI 运行时镜像：二进制由自建 runner 宿主编译（ubuntu 24.04），这里只做组装。
FROM ubuntu:24.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY egress-router /usr/local/bin/egress-router

EXPOSE 3128
ENTRYPOINT ["/usr/local/bin/egress-router", "--config", "/etc/egress/config.toml"]
