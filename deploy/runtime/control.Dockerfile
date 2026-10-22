# CI 运行时镜像：二进制由自建 runner 宿主编译（ubuntu 24.04），这里只做组装。
# 必须与 runner 同 glibc 系；不要改成 debian/alpine。
FROM ubuntu:24.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY web ./web
COPY su8-egress-control /usr/local/bin/su8-egress-control

ENV EGRESS_WEB_DIR=/app/web/dist
ENV EGRESS_BIND=0.0.0.0:8088
EXPOSE 8088
ENTRYPOINT ["/usr/local/bin/su8-egress-control"]
