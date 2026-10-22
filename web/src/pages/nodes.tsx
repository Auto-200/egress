import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, Pencil, Plus, Power, Trash2 } from 'lucide-react';
import { resources } from '@/lib/api';
import type { Node, Region } from '@/lib/types';
import { parseLines } from '@/lib/utils';
import { ErrorBanner, LoadingBlock } from '@/components/feedback';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { ConfirmDialog } from '@/components/ui/confirm-dialog';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { HealthBadge, NodeStatusBadge } from '@/components/status';

type NodeForm = {
  region_id: string;
  name: string;
  endpoint: string;
  server_name: string;
  weight: number;
  capacity: number;
  target_acl: string;
  status: Node['status'];
  cert_fingerprint: string;
};

const emptyForm: NodeForm = {
  region_id: '',
  name: '',
  endpoint: '',
  server_name: '',
  weight: 1,
  capacity: 0,
  target_acl: 'ark.cn-beijing.volces.com:443',
  status: 'active',
  cert_fingerprint: '',
};

function toBody(form: NodeForm) {
  return {
    region_id: form.region_id,
    name: form.name.trim(),
    endpoint: form.endpoint.trim(),
    server_name: form.server_name.trim(),
    transport: 'https-connect',
    weight: form.weight,
    capacity: form.capacity,
    target_acl: parseLines(form.target_acl),
    status: form.status,
    cert_fingerprint: form.cert_fingerprint.trim() || null,
  };
}

function nodeBody(node: Node, status: Node['status']) {
  return {
    region_id: node.region_id,
    name: node.name,
    endpoint: node.endpoint,
    server_name: node.server_name,
    transport: node.transport,
    weight: node.weight,
    capacity: node.capacity,
    target_acl: node.target_acl,
    status,
    cert_fingerprint: node.cert_fingerprint,
  };
}

export function NodesPage() {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [regions, setRegions] = useState<Region[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<Node | null>(null);
  const [form, setForm] = useState<NodeForm>(emptyForm);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const [deleteTarget, setDeleteTarget] = useState<Node | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const [nextNodes, nextRegions] = await Promise.all([resources.nodes(), resources.regions()]);
    setNodes(nextNodes);
    setRegions(nextRegions);
  }, []);

  useEffect(() => {
    let cancelled = false;
    reload()
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : '加载失败');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [reload]);

  const regionName = useMemo(() => {
    const map = new Map(regions.map((region) => [region.id, region.name]));
    return (id: string) => map.get(id) ?? '—';
  }, [regions]);

  const openCreate = () => {
    setEditing(null);
    setForm({ ...emptyForm, region_id: regions[0]?.id ?? '' });
    setFormError(null);
    setDialogOpen(true);
  };

  const openEdit = (node: Node) => {
    setEditing(node);
    setForm({
      region_id: node.region_id,
      name: node.name,
      endpoint: node.endpoint,
      server_name: node.server_name,
      weight: node.weight,
      capacity: node.capacity,
      target_acl: node.target_acl.join('\n'),
      status: node.status,
      cert_fingerprint: node.cert_fingerprint ?? '',
    });
    setFormError(null);
    setDialogOpen(true);
  };

  const submitForm = async () => {
    setSaving(true);
    setFormError(null);
    try {
      if (!form.region_id) throw new Error('请选择地区');
      if (editing) {
        await resources.updateNode(editing.id, toBody(form));
      } else {
        await resources.createNode(toBody(form));
      }
      setDialogOpen(false);
      await reload();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : '保存失败');
    } finally {
      setSaving(false);
    }
  };

  const toggleDrain = async (node: Node) => {
    setBusyId(node.id);
    setError(null);
    try {
      const nextStatus = node.status === 'draining' ? 'active' : 'draining';
      await resources.updateNode(node.id, nodeBody(node, nextStatus));
      await reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : '更新状态失败');
    } finally {
      setBusyId(null);
    }
  };

  const submitDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    setError(null);
    try {
      await resources.deleteNode(deleteTarget.id);
      setDeleteTarget(null);
      await reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除失败');
    } finally {
      setDeleting(false);
    }
  };

  if (loading) return <LoadingBlock />;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="text-sm text-muted-foreground">
          节点是地区 VPS 上的 HTTPS CONNECT 服务；目标 ACL 由节点拉取后生效。
        </div>
        <Button onClick={openCreate}>
          <Plus className="h-4 w-4" />
          新建节点
        </Button>
      </div>
      <ErrorBanner message={error} />

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>节点</TableHead>
                <TableHead>地区</TableHead>
                <TableHead>地址</TableHead>
                <TableHead>健康</TableHead>
                <TableHead>ACL</TableHead>
                <TableHead>线路</TableHead>
                <TableHead>状态</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {nodes.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={8} className="py-12 text-center text-muted-foreground">
                    还没有节点。先创建一台香港节点，再分配到线路。
                  </TableCell>
                </TableRow>
              ) : (
                nodes.map((node) => (
                  <TableRow key={node.id}>
                    <TableCell>
                      <div className="font-medium">{node.name}</div>
                      <div className="text-xs text-muted-foreground">{node.server_name}</div>
                    </TableCell>
                    <TableCell>{regionName(node.region_id)}</TableCell>
                    <TableCell className="font-mono text-xs">{node.endpoint}</TableCell>
                    <TableCell>
                      <HealthBadge
                        healthy={node.healthy}
                        latencyMs={node.latency_ms}
                        detail={node.detail}
                      />
                    </TableCell>
                    <TableCell>
                      <span title={node.target_acl.join('\n')}>
                        {node.target_acl.length} 条
                      </span>
                    </TableCell>
                    <TableCell>{node.line_count}</TableCell>
                    <TableCell>
                      <NodeStatusBadge status={node.status} />
                    </TableCell>
                    <TableCell>
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => toggleDrain(node)}
                          disabled={busyId === node.id}
                        >
                          {busyId === node.id ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            <Power className="h-3.5 w-3.5" />
                          )}
                          {node.status === 'draining' ? '启用' : '排空'}
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => openEdit(node)}>
                          <Pencil className="h-3.5 w-3.5" />
                          编辑
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="text-destructive hover:text-destructive"
                          onClick={() => setDeleteTarget(node)}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{editing ? '编辑节点' : '新建节点'}</DialogTitle>
            <DialogDescription>
              地址必须能被 router 访问；节点证书的 SAN 要和 server_name 一致。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <ErrorBanner message={formError} />
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="node-name">名称</Label>
                <Input
                  id="node-name"
                  value={form.name}
                  placeholder="hk-1"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, name: event.currentTarget.value }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-region">地区</Label>
                <Select
                  id="node-region"
                  value={form.region_id}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      region_id: event.currentTarget.value,
                    }))
                  }
                >
                  <option value="">请选择</option>
                  {regions.map((region) => (
                    <option key={region.id} value={region.id}>
                      {region.code} · {region.name}
                    </option>
                  ))}
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-endpoint">Endpoint</Label>
                <Input
                  id="node-endpoint"
                  value={form.endpoint}
                  placeholder="hk1.egress.example.com:443"
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      endpoint: event.currentTarget.value,
                    }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-server-name">Server name</Label>
                <Input
                  id="node-server-name"
                  value={form.server_name}
                  placeholder="hk1.egress.example.com"
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      server_name: event.currentTarget.value,
                    }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-weight">权重</Label>
                <Input
                  id="node-weight"
                  type="number"
                  min={1}
                  max={1000}
                  value={form.weight}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      weight: Math.max(1, Number(event.currentTarget.value) || 1),
                    }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-capacity">容量（0 = 不限）</Label>
                <Input
                  id="node-capacity"
                  type="number"
                  min={0}
                  value={form.capacity}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      capacity: Math.max(0, Number(event.currentTarget.value) || 0),
                    }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-status">状态</Label>
                <Select
                  id="node-status"
                  value={form.status}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      status: event.currentTarget.value as Node['status'],
                    }))
                  }
                >
                  <option value="active">启用</option>
                  <option value="draining">排空中</option>
                  <option value="disabled">停用</option>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="node-fingerprint">证书指纹（可选）</Label>
                <Input
                  id="node-fingerprint"
                  value={form.cert_fingerprint}
                  placeholder="SHA256:…"
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      cert_fingerprint: event.currentTarget.value,
                    }))
                  }
                />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="node-acl">目标 ACL（每行一个 host:port）</Label>
              <Textarea
                id="node-acl"
                rows={5}
                value={form.target_acl}
                placeholder={'ark.cn-beijing.volces.com:443\napi.moonshot.cn:443'}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    target_acl: event.currentTarget.value,
                  }))
                }
              />
              <p className="text-xs text-muted-foreground">
                空 ACL 会让节点的 `/healthz` 返回 503，router 会把它摘除。
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)} disabled={saving}>
              取消
            </Button>
            <Button
              onClick={submitForm}
              disabled={
                saving ||
                !form.region_id ||
                !form.name.trim() ||
                !form.endpoint.trim() ||
                !form.server_name.trim()
              }
            >
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        title={`删除节点 ${deleteTarget?.name ?? ''}`}
        description="节点会从所有线路移除，router 下一次拉配置后不再使用它。"
        busy={deleting}
        onConfirm={submitDelete}
      />
    </div>
  );
}
