# 协议与失败语义

## 1. Router ↔ 消费者

- 监听：集群内 `:3128`，纯 HTTP（不对外暴露）。
- 方法：`CONNECT host:port HTTP/1.1`。
- 认证：集群内 NetworkPolicy 只放行消费者所在命名空间/Pod；如需代理层用户名密码后续再加。
- 成功：`200` 后进入双向字节转发。
- 失败：
  - 无健康节点：`503`。
  - 节点连接/握手失败：`502`。
  - 节点明确拒绝目标（ACL）：透传 `403`。
- 不允许把失败静默改写成直连。

## 2. Router ↔ Node

- 监听：公网 `:443`，TLS + mTLS。
- 必须校验客户端证书；node 只接受持有项目 CA 签发的 router 证书。
- 方法：`CONNECT host:port HTTP/1.1`。
- 目标 ACL：node 按 `host:port` 精确匹配或 `*.suffix:port` 通配；不匹配返回 `403`。
- DNS：node 本地解析目标域名；router 不解析、不传 IP。
- 成功：`200` 后进入双向字节转发；node 连接目标在返回 200 前完成。
- 失败：
  - 连接目标失败/超时：`502`。
  - ACL 拒绝：`403`。
- node 不终止目标的 TLS，只转发字节；不记录请求体/响应体。

## 3. 健康与指标

### Node

- `GET /livez`（TLS + mTLS）：只检查进程，始终 `200 ok`。
- `GET /healthz`（TLS + mTLS）：进程与监听正常、且 ACL 非空返回 `200 ok`；ACL 为空返回 `503`。
- `GET /metrics`：Prometheus 文本格式。
  - `egress_node_active_connections`
  - `egress_node_connect_total{result="ok|denied|upstream_error"}`
  - `egress_node_upstream_connect_seconds`
  - `egress_node_bytes_total{direction="up|down"}`

### Router

- `GET /livez`：只检查进程，始终 `200 ok`。
- `GET /healthz`：至少一个节点健康返回 `200`，否则 `503`。
- `GET /metrics`：
  - `egress_router_node_healthy{line,node}` 0/1
  - `egress_router_node_active_connections{line,node}`
  - `egress_router_connect_total{line,node,result}`
  - `egress_router_connect_seconds{line}`
  - `egress_router_bytes_total{line,node,direction}`

## 4. 健康检查

- router 每隔 `interval_secs` 对每个 node 做一次：
  1. TCP connect；
  2. TLS 握手（客户端证书）；
  3. `GET /healthz`，期望 `200`。
- 连续 `failure_threshold` 次失败 → 标记不健康；连续 `success_threshold` 次成功 → 恢复。
- 节点初始为不健康，必须通过第一次检查后才接流量。
- `/healthz` 不检查目标 API；目标可达性由实际请求和后续的主动探测补充。

## 5. 超时

- router → node：`connect_timeout_secs`（默认 10s）。
- node → 目标：`connect_timeout_secs`（默认 10s）。
- 流式请求没有额外的空闲超时；由消费者的首包/总超时控制。

## 6. 控制面模式

- Router 启动时向控制面拉 `GET /internal/v1/config/{line}`；控制面返回：
  - `200`：线路配置（节点列表、策略、version），带 `ETag`。
  - `304`：配置未变化。
  - `404`：线路不存在或已停用。
- Router 轮询时带 `If-None-Match`；版本变化才替换节点池。已有连接不打断。
- 控制面不可用时，router 继续用最后一份配置；启动时不可用则以空节点池启动（`/healthz` 503）。
- 控制面明确返回 `404` 时，router 清空节点池（失败关闭），线路恢复后自动重新拉取。
- Node 启动时向控制面拉 `GET /internal/v1/node-config/{node}`；同样使用 `ETag` / `304`。
  - 启动时控制面不可用或节点不存在：以空 ACL 启动（`/healthz` 503，拒绝所有目标）。
  - 运行中控制面返回 `404`：清空 ACL，等待恢复。
- Router 每次健康检查后向 `POST /internal/v1/health/{node}` 上报 `{healthy, latency_ms, detail}`。
- 所有 `/internal/*` 请求必须带 `Authorization: Bearer <EGRESS_INTERNAL_TOKEN>`。
