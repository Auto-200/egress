import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, Pencil, Plus, Trash2, Waypoints } from 'lucide-react';
import { resources } from '@/lib/api';
import type { Line, Node, Region } from '@/lib/types';
import { ErrorBanner, LoadingBlock } from '@/components/feedback';
import { Badge } from '@/components/ui/badge';
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { HealthBadge, LineStatusBadge } from '@/components/status';

type LineForm = {
  code: string;
  name: string;
  region_id: string;
  policy: Line['policy'];
  status: Line['status'];
};

type Assignment = {
  selected: boolean;
  weight: number;
  priority: number;
};

const emptyForm: LineForm = {
  code: '',
  name: '',
  region_id: '',
  policy: 'least-connections',
  status: 'active',
};

function policyLabel(policy: Line['policy']) {
  return policy === 'round-robin' ? '轮询' : '最少连接';
}

export function LinesPage() {
  const [lines, setLines] = useState<Line[]>([]);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [regions, setRegions] = useState<Region[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<Line | null>(null);
  const [form, setForm] = useState<LineForm>(emptyForm);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const [assignLine, setAssignLine] = useState<Line | null>(null);
  const [assignments, setAssignments] = useState<Record<string, Assignment>>({});
  const [assignBusy, setAssignBusy] = useState(false);
  const [assignError, setAssignError] = useState<string | null>(null);

  const [deleteTarget, setDeleteTarget] = useState<Line | null>(null);
  const [deleting, setDeleting] = useState(false);

  const reload = useCallback(async () => {
    const [nextLines, nextNodes, nextRegions] = await Promise.all([
      resources.lines(),
      resources.nodes(),
      resources.regions(),
    ]);
    setLines(nextLines);
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
    return (id: string | null) => (id ? map.get(id) ?? '—' : '—');
  }, [regions]);

  const openCreate = () => {
    setEditing(null);
    setForm({ ...emptyForm, region_id: regions[0]?.id ?? '' });
    setFormError(null);
    setDialogOpen(true);
  };

  const openEdit = (line: Line) => {
    setEditing(line);
    setForm({
      code: line.code,
      name: line.name,
      region_id: line.region_id ?? '',
      policy: line.policy,
      status: line.status,
    });
    setFormError(null);
    setDialogOpen(true);
  };

  const submitForm = async () => {
    setSaving(true);
    setFormError(null);
    try {
      const body = {
        code: form.code.trim(),
        name: form.name.trim(),
        region_id: form.region_id || null,
        policy: form.policy,
        status: form.status,
      };
      if (editing) {
        await resources.updateLine(editing.id, body);
      } else {
        await resources.createLine(body);
      }
      setDialogOpen(false);
      await reload();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : '保存失败');
    } finally {
      setSaving(false);
    }
  };

  const openAssign = async (line: Line) => {
    setAssignLine(line);
    setAssignError(null);
    setAssignBusy(true);
    try {
      const detail = await resources.line(line.id);
      const next: Record<string, Assignment> = {};
      for (const node of nodes) {
        next[node.id] = { selected: false, weight: 1, priority: 100 };
      }
      for (const assigned of detail.nodes) {
        next[assigned.id] = {
          selected: true,
          weight: assigned.weight,
          priority: assigned.priority,
        };
      }
      setAssignments(next);
    } catch (err) {
      setAssignError(err instanceof Error ? err.message : '加载分配失败');
    } finally {
      setAssignBusy(false);
    }
  };

  const submitAssign = async () => {
    if (!assignLine) return;
    setAssignBusy(true);
    setAssignError(null);
    try {
      const payload = Object.entries(assignments)
        .filter(([, value]) => value.selected)
        .map(([nodeId, value]) => ({
          node_id: nodeId,
          weight: value.weight,
          priority: value.priority,
        }));
      await resources.setLineNodes(assignLine.id, payload);
      setAssignLine(null);
      await reload();
    } catch (err) {
      setAssignError(err instanceof Error ? err.message : '保存分配失败');
    } finally {
      setAssignBusy(false);
    }
  };

  const submitDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    setError(null);
    try {
      await resources.deleteLine(deleteTarget.id);
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
          线路是 router 的稳定入口；节点通过“分配节点”加入线路。
        </div>
        <Button onClick={openCreate}>
          <Plus className="h-4 w-4" />
          新建线路
        </Button>
      </div>
      <ErrorBanner message={error} />

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>线路</TableHead>
                <TableHead>地区</TableHead>
                <TableHead>策略</TableHead>
                <TableHead>节点</TableHead>
                <TableHead>状态</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {lines.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={6} className="py-12 text-center text-muted-foreground">
                    还没有线路。
                  </TableCell>
                </TableRow>
              ) : (
                lines.map((line) => (
                  <TableRow key={line.id}>
                    <TableCell>
                      <div className="font-medium">{line.code}</div>
                      <div className="text-xs text-muted-foreground">{line.name}</div>
                    </TableCell>
                    <TableCell>{line.region_name ?? regionName(line.region_id)}</TableCell>
                    <TableCell>{policyLabel(line.policy)}</TableCell>
                    <TableCell>
                      <span className="font-medium">{line.healthy_node_count}</span>
                      <span className="text-muted-foreground"> / {line.node_count} 健康</span>
                    </TableCell>
                    <TableCell>
                      <LineStatusBadge status={line.status} />
                    </TableCell>
                    <TableCell>
                      <div className="flex justify-end gap-1">
                        <Button variant="ghost" size="sm" onClick={() => openAssign(line)}>
                          <Waypoints className="h-3.5 w-3.5" />
                          分配
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => openEdit(line)}>
                          <Pencil className="h-3.5 w-3.5" />
                          编辑
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="text-destructive hover:text-destructive"
                          onClick={() => setDeleteTarget(line)}
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
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing ? '编辑线路' : '新建线路'}</DialogTitle>
            <DialogDescription>线路的 code 是 router 拉取配置时使用的标识。</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <ErrorBanner message={formError} />
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="line-code">Code</Label>
                <Input
                  id="line-code"
                  value={form.code}
                  placeholder="hk"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, code: event.currentTarget.value }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="line-name">名称</Label>
                <Input
                  id="line-name"
                  value={form.name}
                  placeholder="香港出口"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, name: event.currentTarget.value }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="line-region">地区</Label>
                <Select
                  id="line-region"
                  value={form.region_id}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      region_id: event.currentTarget.value,
                    }))
                  }
                >
                  <option value="">不指定</option>
                  {regions.map((region) => (
                    <option key={region.id} value={region.id}>
                      {region.code} · {region.name}
                    </option>
                  ))}
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="line-policy">选路策略</Label>
                <Select
                  id="line-policy"
                  value={form.policy}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      policy: event.currentTarget.value as Line['policy'],
                    }))
                  }
                >
                  <option value="least-connections">最少连接</option>
                  <option value="round-robin">轮询</option>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="line-status">状态</Label>
                <Select
                  id="line-status"
                  value={form.status}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      status: event.currentTarget.value as Line['status'],
                    }))
                  }
                >
                  <option value="active">启用</option>
                  <option value="disabled">停用</option>
                </Select>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)} disabled={saving}>
              取消
            </Button>
            <Button onClick={submitForm} disabled={saving || !form.code.trim() || !form.name.trim()}>
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={assignLine !== null}
        onOpenChange={(open) => {
          if (!open) setAssignLine(null);
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>分配节点 · {assignLine?.code}</DialogTitle>
            <DialogDescription>
              只有勾选且节点状态为启用的节点会进入 router 的配置。
            </DialogDescription>
          </DialogHeader>
          <ErrorBanner message={assignError} />
          {assignBusy && Object.keys(assignments).length === 0 ? (
            <LoadingBlock label="加载节点…" />
          ) : nodes.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted-foreground">
              还没有节点，请先在“节点”页面创建。
            </div>
          ) : (
            <div className="max-h-[52vh] space-y-2 overflow-auto pr-1">
              {nodes.map((node) => {
                const assignment = assignments[node.id] ?? {
                  selected: false,
                  weight: 1,
                  priority: 100,
                };
                return (
                  <div
                    key={node.id}
                    className="flex flex-wrap items-center gap-3 rounded-lg border px-3 py-2.5"
                  >
                    <input
                      type="checkbox"
                      className="h-4 w-4 rounded border-input accent-primary"
                      checked={assignment.selected}
                      onChange={(event) =>
                        setAssignments((current) => ({
                          ...current,
                          [node.id]: {
                            ...assignment,
                            selected: event.currentTarget.checked,
                          },
                        }))
                      }
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium">{node.name}</span>
                        <HealthBadge
                          healthy={node.healthy}
                          latencyMs={node.latency_ms}
                          detail={node.detail}
                        />
                      </div>
                      <div className="truncate text-xs text-muted-foreground">
                        {node.region_code} · {node.endpoint}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Label className="text-xs text-muted-foreground" htmlFor={`w-${node.id}`}>
                        权重
                      </Label>
                      <Input
                        id={`w-${node.id}`}
                        type="number"
                        min={1}
                        max={1000}
                        className="h-8 w-20"
                        value={assignment.weight}
                        onChange={(event) =>
                          setAssignments((current) => ({
                            ...current,
                            [node.id]: {
                              ...assignment,
                              weight: Math.max(1, Number(event.currentTarget.value) || 1),
                            },
                          }))
                        }
                      />
                      <Label className="text-xs text-muted-foreground" htmlFor={`p-${node.id}`}>
                        优先级
                      </Label>
                      <Input
                        id={`p-${node.id}`}
                        type="number"
                        min={0}
                        max={100000}
                        className="h-8 w-24"
                        value={assignment.priority}
                        onChange={(event) =>
                          setAssignments((current) => ({
                            ...current,
                            [node.id]: {
                              ...assignment,
                              priority: Math.max(0, Number(event.currentTarget.value) || 0),
                            },
                          }))
                        }
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={() => setAssignLine(null)} disabled={assignBusy}>
              取消
            </Button>
            <Button onClick={submitAssign} disabled={assignBusy}>
              {assignBusy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              保存分配
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        title={`删除线路 ${deleteTarget?.code ?? ''}`}
        description="线路删除后，router 会拿到 404 并清空该线路的节点（失败关闭）。"
        busy={deleting}
        onConfirm={submitDelete}
      />

      {nodes.length === 0 ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Badge variant="outline">提示</Badge>
          先去“节点”页面添加一台香港节点，再回来分配。
        </div>
      ) : null}
    </div>
  );
}
