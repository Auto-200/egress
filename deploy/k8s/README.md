# Kubernetes 部署

1. 镜像由 `.github/workflows/release-images.yml` 在自建 runner 上构建并推送：
   - `Dockerfile.control` → `ghcr.io/auto-200/egress-control:<tag>`
   - `Dockerfile.router` → `ghcr.io/auto-200/egress-router:<tag>`
   - `Dockerfile.node` → `ghcr.io/auto-200/egress-node:<tag>`

2. 准备控制面 Secret：
   - `DATABASE_URL`：独立库，不要和其他业务共用 schema。
   - `EGRESS_ADMIN_PASSWORD` / `EGRESS_INTERNAL_TOKEN`。

3. 应用控制面：

```bash
kubectl apply -f deploy/k8s/control.yaml
```

4. 准备 router 的证书 Secret（`egress-router-tls`）：
   - `ca.crt`：签发 node 与 router 证书的 CA。
   - `router.crt` / `router.key`：router 的客户端证书，SAN/用途按 CA 要求。
   - 同时把 `egress-router-config` 里的 token 换成真实值。

5. 应用 router：

```bash
kubectl apply -f deploy/k8s/router.yaml
```

6. 消费者接入：把业务侧的代理地址填成

```text
http://egress-router-hk.egress.svc:3128
```

只改代理配置，不改业务代码。

注意：

- `router.yaml` 里的 NetworkPolicy 使用 `app.kubernetes.io/name=my-consumer` 作为示例；
  按实际消费者的 Pod 标签修改后再应用，否则业务 Pod 会被挡住。
- 控制面目前是单副本；生产建议至少 2 副本并把 Postgres 放在可靠存储上。
- router 的 `/healthz` 在没有健康节点时返回 503，readiness 会把它摘除；
  `/livez` 只检查进程。
