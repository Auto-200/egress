# su8-egress

独立的多地区出口网络。项目完全在本仓库开发；SU8 只是它的一个消费者，通过现有的
`channel_members.egress`（HTTP / HTTPS / SOCKS 代理地址）接入，不需要改 SU8 的 gateway 代码。

## 边界

- 本仓库：控制面、管理端、router、node、协议、部署。
- SU8：只保存一个代理地址，例如 `http://egress-router.egress.svc:3128`；
  它不知道节点、地区、transport，也不参与本项目的发布。
- 兼容面：标准 HTTP CONNECT 代理。SU8 已支持 `http` / `https` / `socks5` / `socks5h`；
  首期用 `http://`（集群内）或 `https://`（公网）即可。

## 组件

| 组件 | 位置 | 作用 |
| --- | --- | --- |
| `egress-control` | 集群内 | 线路、节点、分配、健康上报、配置分发、审计；自带管理 UI |
| `web` | 控制面静态资源 | shadcn/ui 管理端：总览、线路、节点、地区、审计 |
| `egress-router` | 集群内 | HTTP CONNECT 入口：拉线路配置、选健康节点、mTLS 到节点、故障摘除、指标 |
| `egress-node` | 地区 VPS | HTTPS CONNECT 服务：mTLS、目标域名白名单、远端 DNS、健康/指标 |
| SU8 gateway | 外部消费者 | 成员 `egress` 指向 router 或 node 的代理地址 |

## 数据流

```text
gateway --HTTP CONNECT--> egress-router --HTTPS CONNECT (mTLS)--> egress-node --> API
                                │
                                └── 拉配置 / 上报节点健康 ──> egress-control
```

- router 对 gateway 是无 TLS 的集群内 HTTP 代理；由 NetworkPolicy 限制访问。
- router 到 node 是 TLS + mTLS，node 只接受持有客户端证书的 router。
- API 的 TLS 是端到端的：node 只看到目标域名和加密字节，不终止 API TLS。
- node 在本地区做 DNS 解析，不拿美国解析出来的 IP。
- 控制面不可用时：router 用最后一份配置继续跑；node 用最后一次 ACL 继续跑。
  控制面明确返回 404（线路/节点被停用或删除）时，router 清空节点池、node 清空 ACL（失败关闭）。

## 快速开始

### 1. 数据库

使用公共开发栈的 PostgreSQL（一项目一角色一库）：

```bash
createuser -h 127.0.0.1 -U night su8_egress
createdb -h 127.0.0.1 -U night -O su8_egress su8_egress
```

### 2. 控制面

```bash
cp .env.example .env
# 修改 EGRESS_ADMIN_PASSWORD 和 EGRESS_INTERNAL_TOKEN
cargo run -p egress-control
```

控制面启动时自动跑 migration 并 seed `hk` / `cn` / `usw` 三个地区和线路。
管理端在 `http://127.0.0.1:8088`。

### 3. 管理端

```bash
cd web
bun install
bun run build
```

构建产物在 `web/dist`，控制面会直接托管。开发时可以 `bun run dev`（Vite 代理到 `:8088`）。

### 4. 本地 node + router（文件配置）

```bash
./scripts/gen-dev-certs.sh
cargo run -p egress-node   -- --config deploy/node/config.dev.toml
cargo run -p egress-router -- --config deploy/router/config.dev.toml
curl -x http://127.0.0.1:3128 -I https://example.com
```

### 5. 控制面模式

1. 在管理端“节点”里创建节点，填 endpoint / server_name / 目标 ACL。
2. 在“线路”里把节点分配到线路。
3. 在 VPS 上用 `deploy/node/config.control.toml` 启动节点（token 与 `EGRESS_INTERNAL_TOKEN` 一致）。
4. router 用 `deploy/router/config.control.toml` 启动，配置从控制面拉取。
5. SU8 把成员的 `egress_proxy_url` 填成 router 的地址。

## 配置

- 控制面：`.env.example`
- 节点：`deploy/node/config.example.toml`（文件模式）、`deploy/node/config.control.toml`（控制面模式）
- Router：`deploy/router/config.example.toml`（文件模式）、`deploy/router/config.control.toml`（控制面模式）
- 生产节点的 TLS 证书必须包含 router 使用的 `server_name`；node 的 `client_ca` 是必填项。

## 测试与发布

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && bun install && bun run build
```

以上命令与 `.github/workflows/ci.yml` 一致，CI 跑在组织自建 runner
（`self-hosted, linux, x64, auto200`）上。所有 push 与 PR 都会执行 fmt / clippy / test / 管理端构建。

镜像发布由 `.github/workflows/release-images.yml` 负责：

- 推 `main`：构建并推送 `latest` 与 `sha-<12位>`。
- 推 `v*` tag：额外推送该 tag。
- 也可在 Actions 里手动 `workflow_dispatch`。

镜像地址：

```text
ghcr.io/auto-200/su8-egress-control
ghcr.io/auto-200/su8-egress-router
ghcr.io/auto-200/su8-egress-node
```

K8s 清单与部署步骤见 `deploy/k8s/README.md`。工作流只负责构建和推送镜像；
真正的集群发布仍需按变更窗口手动执行。

## 当前状态

已实现：控制面 API + Postgres migration + 管理 UI（shadcn/ui）、router/node 双模式
（文件/控制面）、健康检查与上报、配置 ETag/304、审计、失败关闭、Dockerfile、K8s 清单、CI。

尚未实现：节点注册/证书自动签发与轮换、主动目标探测与结果持久化、WireGuard/SOCKS5 transport、
配置版本回滚、多实例控制面/高可用、控制面 API 的集成测试。

没有静默回落直连；线路不可用时由 router 返回 5xx，由上层决定是否切号。
