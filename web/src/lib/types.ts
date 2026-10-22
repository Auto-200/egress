export type User = {
  user: string;
};

export type Region = {
  id: string;
  code: string;
  name: string;
  country: string | null;
  status: 'active' | 'disabled';
  created_at: string;
};

export type Node = {
  id: string;
  region_id: string;
  region_code: string;
  region_name: string;
  name: string;
  endpoint: string;
  server_name: string;
  transport: 'https-connect';
  weight: number;
  capacity: number;
  target_acl: string[];
  status: 'active' | 'disabled' | 'draining';
  cert_fingerprint: string | null;
  created_at: string;
  updated_at: string;
  healthy: boolean | null;
  checked_at: string | null;
  latency_ms: number | null;
  detail: string | null;
  line_count: number;
};

export type Line = {
  id: string;
  code: string;
  name: string;
  region_id: string | null;
  region_code: string | null;
  region_name: string | null;
  policy: 'least-connections' | 'round-robin';
  status: 'active' | 'disabled';
  node_count: number;
  healthy_node_count: number;
  created_at: string;
  updated_at: string;
};

export type LineNode = {
  id: string;
  name: string;
  endpoint: string;
  server_name: string;
  status: 'active' | 'disabled' | 'draining';
  weight: number;
  priority: number;
  healthy: boolean | null;
  checked_at: string | null;
  latency_ms: number | null;
};

export type LineDetail = {
  line: Line;
  nodes: LineNode[];
};

export type AuditEvent = {
  id: number;
  actor: string;
  action: string;
  object: string;
  detail: unknown;
  created_at: string;
};

export type Overview = {
  regions: number;
  lines: number;
  nodes: number;
  healthy_nodes: number;
  unhealthy_nodes: number;
  active_lines: number;
  recent_audit: AuditEvent[];
};
