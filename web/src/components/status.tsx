import { Badge } from '@/components/ui/badge';

export function LineStatusBadge({ status }: { status: 'active' | 'disabled' }) {
  return status === 'active' ? (
    <Badge variant="success">启用</Badge>
  ) : (
    <Badge variant="secondary">停用</Badge>
  );
}

export function NodeStatusBadge({
  status,
}: {
  status: 'active' | 'disabled' | 'draining';
}) {
  if (status === 'active') return <Badge variant="success">启用</Badge>;
  if (status === 'draining') return <Badge variant="warning">排空中</Badge>;
  return <Badge variant="secondary">停用</Badge>;
}

export function HealthBadge({
  healthy,
  latencyMs,
  detail,
}: {
  healthy: boolean | null;
  latencyMs?: number | null;
  detail?: string | null;
}) {
  if (healthy === true) {
    return (
      <span title={detail ?? undefined}>
        <Badge variant="success">
          健康{latencyMs != null ? ` · ${latencyMs}ms` : ''}
        </Badge>
      </span>
    );
  }
  if (healthy === false) {
    return (
      <span title={detail ?? undefined}>
        <Badge variant="destructive">异常</Badge>
      </span>
    );
  }
  return (
    <span title={detail ?? 'router 尚未上报健康'}>
      <Badge variant="outline">未上报</Badge>
    </span>
  );
}
