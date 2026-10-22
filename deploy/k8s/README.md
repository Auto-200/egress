# Kubernetes 部署

1. 镜像由 `.github/workflows/release-images.yml` 在自建 runner 上构建并推送：
   - `Dockerfile.control` → `ghcr.io/auto-200/su8-egress-control:<tag>`
   - `Dockerfile.router` → `ghcr.io/auto-200/su8-egress-router:<tag>`
   - `Dockerfile.node` → `ghcr.io/auto-200/su8-egress-node:<tag>`

2. 准备控制面 Secret：
   - `DATABASE_URL`：独立库（不要和 SU8 主库共用 schema）。
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

6. SU8 集成：把成员的 `egress_proxy_url` 填成

```text
http://egress-router-hk.su8-egress.svc:3128
```

只改成员配置，不改 gateway 代码。

注意：

- `router.yaml` 里的 NetworkPolicy 使用 `app.kubernetes.io/name=su8-gateway` 作为示例；
  如果 SU8 的 Pod 标签不同，先改策略再应用，否则 gateway 会被挡住。
- 控制面目前是单副本；生产建议至少 2 副本并把 Postgres 放在可靠存储上。
- router 的 `/healthz` 在没有健康节点时返回 503，readiness 会把它摘除；
  `/livez` 只检查进程。
