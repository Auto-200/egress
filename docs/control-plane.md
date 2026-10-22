# 控制面

控制面在本仓库内，不进入 SU8。SU8 只是消费者：管理端保存一个代理地址，
gateway 通过现有 `channel_members.egress` 使用它。

实现位置：`crates/egress-control`（API + 静态管理端）、`web/`（shadcn/ui 管理端）。

## 数据模型（已实现）

migration：`crates/egress-control/migrations/0001_init.sql`

- `regions`：`id, code, name, country, status(active|disabled), created_at`
- `nodes`：`id, region_id, name, endpoint, server_name, transport(https-connect), weight, capacity, target_acl(jsonb), status(active|disabled|draining), cert_fingerprint, created_at, updated_at`
- `lines`：`id, code, name, region_id, policy(least-connections|round-robin), status(active|disabled), created_at, updated_at`
- `line_nodes`：`line_id, node_id, weight, priority`
- `node_health`：`node_id, healthy, checked_at, checked_by, latency_ms, detail`
- `audit_events`：`id, actor, action, object, detail(jsonb), created_at`
- `admin_sessions`：`token_hash, created_at, expires_at`

启动时 seed 三个地区与线路：`hk` / `cn` / `usw`。

## 管理 API（已实现）

前缀 `/api/v1`，除登录外都要求会话 Cookie。

- `POST /auth/login`、`GET /auth/me`、`POST /auth/logout`
- `GET /overview`：节点/线路/地区数量、健康节点数、最近审计
- `GET|POST /regions`、`PATCH|DELETE /regions/{id}`
- `GET|POST /nodes`、`PATCH|DELETE /nodes/{id}`
- `GET|POST /lines`、`GET|PATCH|DELETE /lines/{id}`
- `PUT /lines/{id}/nodes`：整体替换线路的节点分配
- `GET /audit?limit=`：审计事件

所有写操作记录 `audit_events`；删除被引用的地区或节点由外键拒绝。

## 内部 API（已实现）

前缀 `/internal`，要求 `Authorization: Bearer <EGRESS_INTERNAL_TOKEN>`。

- `GET /v1/config/{line}`：router 拉取线路配置，返回 `RouterLineConfig`，带 `ETag`；
  `If-None-Match` 命中返回 `304`；线路不存在或停用返回 `404`。
- `GET /v1/node-config/{node}`：node 拉取目标 ACL，返回 `NodeAclConfig`，带 `ETag`；
  节点不存在或停用返回 `404`。
- `POST /v1/health/{node}`：router 上报 `{healthy, latency_ms, detail}`，写入 `node_health`。

## 认证

- 管理端：单管理员账号，密码来自 `EGRESS_ADMIN_PASSWORD`（启动时用 argon2 哈希）
  或预生成的 `EGRESS_ADMIN_PASSWORD_HASH`。
- 会话：随机 token 放入 `egress_session` Cookie（HttpOnly、SameSite=Lax，可选 Secure），
  数据库只存 SHA-256 哈希，过期时间 `EGRESS_SESSION_TTL_SECS`。
- 内部 API：共享 token，常量时间比较。

## 配置分发

- router 每 `poll_interval_secs` 拉一次线路配置，带 ETag；变更原子替换节点池，旧连接继续跑。
- 控制面不可用时 router 使用最后一份配置；本地健康检查继续。
- 控制面返回 404 时清空节点池（失败关闭）；线路恢复后自动重新拉取。
- node 同样按 ETag 拉取 ACL；控制面不可用时沿用最后一次 ACL，404 时清空 ACL。
- 配置变更写审计。

## 消费者接入

- 每个消费者拿到一个 line 的代理地址，例如：
  - 集群内：`http://egress-router.egress.svc:3128`
  - 公网：`https://hk-egress.example.com:443`
- SU8：在 `channel_members.egress` 里保存该地址；不需要知道线路/节点/transport。
- 多消费者隔离：不同 line 或不同的 router Service；后续可加 Basic Auth / 每消费者 token。

## 尚未实现

- 节点注册/证书自动签发与轮换（当前证书由 `scripts/gen-dev-certs.sh` 或部署侧提供）
- 主动目标探测与 `probe_results` 持久化（当前只有 router 对 node 的健康检查）
- 配置版本化回滚（当前以数据库当前状态为准）
- 多实例控制面/高可用
