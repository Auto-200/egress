import { useCallback, useEffect, useState } from 'react';
import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { resources } from '@/lib/api';
import type { Region } from '@/lib/types';
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

type RegionForm = {
  code: string;
  name: string;
  country: string;
  status: Region['status'];
};

const emptyForm: RegionForm = {
  code: '',
  name: '',
  country: '',
  status: 'active',
};

export function RegionsPage() {
  const [regions, setRegions] = useState<Region[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<Region | null>(null);
  const [form, setForm] = useState<RegionForm>(emptyForm);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Region | null>(null);
  const [deleting, setDeleting] = useState(false);

  const reload = useCallback(async () => {
    setRegions(await resources.regions());
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

  const openCreate = () => {
    setEditing(null);
    setForm(emptyForm);
    setFormError(null);
    setDialogOpen(true);
  };

  const openEdit = (region: Region) => {
    setEditing(region);
    setForm({
      code: region.code,
      name: region.name,
      country: region.country ?? '',
      status: region.status,
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
        country: form.country.trim() || null,
        status: form.status,
      };
      if (editing) {
        await resources.updateRegion(editing.id, body);
      } else {
        await resources.createRegion(body);
      }
      setDialogOpen(false);
      await reload();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : '保存失败');
    } finally {
      setSaving(false);
    }
  };

  const submitDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    setError(null);
    try {
      await resources.deleteRegion(deleteTarget.id);
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
        <div className="text-sm text-muted-foreground">地区是线路和节点的目录，不直接承载流量。</div>
        <Button onClick={openCreate}>
          <Plus className="h-4 w-4" />
          新建地区
        </Button>
      </div>
      <ErrorBanner message={error} />

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Code</TableHead>
                <TableHead>名称</TableHead>
                <TableHead>国家/区域</TableHead>
                <TableHead>状态</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {regions.map((region) => (
                <TableRow key={region.id}>
                  <TableCell className="font-mono text-xs">{region.code}</TableCell>
                  <TableCell className="font-medium">{region.name}</TableCell>
                  <TableCell>{region.country ?? '—'}</TableCell>
                  <TableCell>
                    {region.status === 'active' ? (
                      <Badge variant="success">启用</Badge>
                    ) : (
                      <Badge variant="secondary">停用</Badge>
                    )}
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-1">
                      <Button variant="ghost" size="sm" onClick={() => openEdit(region)}>
                        <Pencil className="h-3.5 w-3.5" />
                        编辑
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-destructive hover:text-destructive"
                        onClick={() => setDeleteTarget(region)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing ? '编辑地区' : '新建地区'}</DialogTitle>
            <DialogDescription>code 用于线路和节点的归属展示。</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <ErrorBanner message={formError} />
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="region-code">Code</Label>
                <Input
                  id="region-code"
                  value={form.code}
                  placeholder="hk"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, code: event.currentTarget.value }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="region-name">名称</Label>
                <Input
                  id="region-name"
                  value={form.name}
                  placeholder="香港"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, name: event.currentTarget.value }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="region-country">国家/区域</Label>
                <Input
                  id="region-country"
                  value={form.country}
                  placeholder="HK"
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      country: event.currentTarget.value,
                    }))
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="region-status">状态</Label>
                <Select
                  id="region-status"
                  value={form.status}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      status: event.currentTarget.value as Region['status'],
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

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        title={`删除地区 ${deleteTarget?.code ?? ''}`}
        description="如果还有节点或线路引用这个地区，服务端会拒绝删除。"
        busy={deleting}
        onConfirm={submitDelete}
      />
    </div>
  );
}
