import { useEffect, useState } from 'react';
import {
  Activity,
  CircleAlert,
  CircleCheck,
  Route as RouteIcon,
  Server,
} from 'lucide-react';
import { resources } from '@/lib/api';
import type { AuditEvent, Line, Overview } from '@/lib/types';
import { formatTime } from '@/lib/utils';
import { ErrorBanner, LoadingBlock } from '@/components/feedback';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { LineStatusBadge } from '@/components/status';

function policyLabel(policy: Line['policy']) {
  return policy === 'round-robin' ? '轮询' : '最少连接';
}

function Kpi({
  label,
  value,
  hint,
  icon,
  tone = 'default',
}: {
  label: string;
  value: number;
  hint: string;
  icon: React.ReactNode;
  tone?: 'default' | 'success' | 'danger';
}) {
  const toneClass =
    tone === 'success'
      ? 'bg-success/10 text-success'
      : tone === 'danger'
        ? 'bg-destructive/10 text-destructive'
        : 'bg-primary/10 text-primary';
  return (
    <Card>
      <CardContent className="flex items-start justify-between p-5">
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            {label}
          </div>
          <div className="mt-2 text-2xl font-semibold tracking-tight">{value}</div>
          <div className="mt-1 text-xs text-muted-foreground">{hint}</div>
        </div>
        <div className={`flex h-9 w-9 items-center justify-center rounded-lg ${toneClass}`}>
          {icon}
        </div>
      </CardContent>
    </Card>
  );
}

function AuditList({ events }: { events: AuditEvent[] }) {
  if (events.length === 0) {
    return <div className="py-6 text-sm text-muted-foreground">还没有变更记录。</div>;
  }
  return (
    <div className="space-y-3">
      {events.map((event) => (
        <div key={event.id} className="flex items-start justify-between gap-3 text-sm">
          <div className="min-w-0">
            <div className="truncate font-medium">{event.action}</div>
            <div className="truncate text-xs text-muted-foreground">
              {event.object} · {event.actor}
            </div>
          </div>
          <div className="shrink-0 text-xs text-muted-foreground">
            {formatTime(event.created_at)}
          </div>
        </div>
      ))}
    </div>
  );
}

export function DashboardPage() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [lines, setLines] = useState<Line[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [nextOverview, nextLines] = await Promise.all([
          resources.overview(),
          resources.lines(),
        ]);
        if (cancelled) return;
        setOverview(nextOverview);
        setLines(nextLines);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : '加载失败');
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) return <LoadingBlock />;

  return (
    <div className="space-y-6">
      <ErrorBanner message={error} />
      {overview ? (
        <>
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <Kpi
              label="线路"
              value={overview.lines}
              hint={`${overview.active_lines} 条启用`}
              icon={<RouteIcon className="h-4 w-4" />}
            />
            <Kpi
              label="节点"
              value={overview.nodes}
              hint="所有地区出口节点"
              icon={<Server className="h-4 w-4" />}
            />
            <Kpi
              label="健康节点"
              value={overview.healthy_nodes}
              hint="router 最近上报为健康"
              tone="success"
              icon={<CircleCheck className="h-4 w-4" />}
            />
            <Kpi
              label="异常节点"
              value={overview.unhealthy_nodes}
              hint="未上报或最近上报失败"
              tone={overview.unhealthy_nodes > 0 ? 'danger' : 'default'}
              icon={<CircleAlert className="h-4 w-4" />}
            />
          </div>

          {overview.unhealthy_nodes > 0 ? (
            <div className="flex items-start gap-2 rounded-xl border border-warning/30 bg-warning/5 px-4 py-3 text-sm text-warning">
              <CircleAlert className="mt-0.5 h-4 w-4 shrink-0" />
              有 {overview.unhealthy_nodes} 个节点异常；检查节点进程、mTLS 证书和 router 上报。
            </div>
          ) : null}

          <div className="grid gap-4 xl:grid-cols-[1.6fr_1fr]">
            <Card>
              <CardHeader className="flex-row items-center justify-between">
                <CardTitle className="flex items-center gap-2">
                  <Activity className="h-4 w-4 text-muted-foreground" />
                  线路健康
                </CardTitle>
                <Badge variant="outline">{lines.length} 条</Badge>
              </CardHeader>
              <CardContent className="p-0">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>线路</TableHead>
                      <TableHead>地区</TableHead>
                      <TableHead>策略</TableHead>
                      <TableHead>节点</TableHead>
                      <TableHead>状态</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {lines.map((line) => (
                      <TableRow key={line.id}>
                        <TableCell>
                          <div className="font-medium">{line.code}</div>
                          <div className="text-xs text-muted-foreground">{line.name}</div>
                        </TableCell>
                        <TableCell>{line.region_name ?? '—'}</TableCell>
                        <TableCell>{policyLabel(line.policy)}</TableCell>
                        <TableCell>
                          <span className="font-medium">{line.healthy_node_count}</span>
                          <span className="text-muted-foreground"> / {line.node_count}</span>
                        </TableCell>
                        <TableCell>
                          <LineStatusBadge status={line.status} />
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>最近变更</CardTitle>
              </CardHeader>
              <CardContent>
                <AuditList events={overview.recent_audit} />
              </CardContent>
            </Card>
          </div>
        </>
      ) : null}
    </div>
  );
}
